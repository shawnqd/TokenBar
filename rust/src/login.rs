//! Login flow runners for various providers
//!
//! Runs CLI login commands and captures output/URLs

#![allow(dead_code)]

use regex_lite::Regex;
use std::io::{BufRead, BufReader, Read};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// Result of a login attempt
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub outcome: LoginOutcome,
    pub output: String,
    pub auth_link: Option<String>,
}

/// Outcome of login attempt
#[derive(Debug, Clone)]
pub enum LoginOutcome {
    Success,
    TimedOut,
    Failed { status: i32 },
    MissingBinary,
    LaunchFailed(String),
}

/// Phase of the login process
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoginPhase {
    Idle,
    Requesting,
    WaitingBrowser,
    Complete,
}

/// Run Claude CLI login
pub async fn run_claude_login<F>(timeout_secs: u64, on_phase: F) -> LoginResult
where
    F: Fn(LoginPhase) + Send + 'static,
{
    run_cli_login(
        "claude",
        &["/login"],
        timeout_secs,
        on_phase,
        &[
            "Successfully logged in",
            "Login successful",
            "Logged in successfully",
        ],
    )
    .await
}

/// Run Codex CLI login
pub async fn run_codex_login<F>(timeout_secs: u64, on_phase: F) -> LoginResult
where
    F: Fn(LoginPhase) + Send + 'static,
{
    run_cli_login(
        "codex",
        &["auth", "login"],
        timeout_secs,
        on_phase,
        &[
            "Successfully logged in",
            "Login successful",
            "Logged in successfully",
        ],
    )
    .await
}

/// Run Gemini/gcloud login
pub async fn run_gemini_login<F>(timeout_secs: u64, on_phase: F) -> LoginResult
where
    F: Fn(LoginPhase) + Send + 'static,
{
    run_cli_login(
        "gcloud",
        &["auth", "login"],
        timeout_secs,
        on_phase,
        &["You are now logged in", "Credentials saved"],
    )
    .await
}

/// Run Copilot/GitHub device flow login
pub async fn run_copilot_login<F>(timeout_secs: u64, on_phase: F) -> LoginResult
where
    F: Fn(LoginPhase) + Send + 'static,
{
    run_cli_login(
        "gh",
        &["auth", "login", "-w"],
        timeout_secs,
        on_phase,
        &["Logged in as", "Authentication complete"],
    )
    .await
}

/// Generic CLI login runner
async fn run_cli_login<F>(
    binary: &str,
    args: &[&str],
    timeout_secs: u64,
    on_phase: F,
    success_markers: &[&str],
) -> LoginResult
where
    F: Fn(LoginPhase) + Send + 'static,
{
    let binary_path = match which::which(binary) {
        Ok(p) => p,
        Err(_) => return missing_binary_result(binary),
    };

    on_phase(LoginPhase::Requesting);

    let mut child = match spawn_login_process(binary_path.as_path(), args) {
        Ok(c) => c,
        Err(e) => return launch_failed_result(e),
    };

    let mut state = CliLoginState::new(timeout_secs, &on_phase, success_markers);
    let (tx, rx) = mpsc::channel::<String>();
    spawn_login_reader(child.stdout.take(), tx.clone());
    spawn_login_reader(child.stderr.take(), tx);

    // Read stdout and stderr concurrently. The previous sequential reads could
    // wait forever on an open stdout pipe while the CLI was waiting for a
    // browser callback on stderr, so the advertised login timeout never fired.
    loop {
        let remaining = state.timeout.saturating_sub(state.start.elapsed());
        match rx.recv_timeout(remaining) {
            Ok(line) => {
                if let Some(outcome) = state.handle_line(&line) {
                    return stop_child_with_outcome(&mut child, state, outcome);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return stop_child_with_outcome(&mut child, state, LoginOutcome::TimedOut);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    wait_for_login_exit(child, state, &on_phase)
}

fn missing_binary_result(binary: &str) -> LoginResult {
    LoginResult {
        outcome: LoginOutcome::MissingBinary,
        output: format!("{} not found in PATH", binary),
        auth_link: None,
    }
}

fn launch_failed_result(error: String) -> LoginResult {
    LoginResult {
        outcome: LoginOutcome::LaunchFailed(error),
        output: String::new(),
        auth_link: None,
    }
}

fn spawn_login_process(binary_path: &std::path::Path, args: &[&str]) -> Result<Child, String> {
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new(binary_path);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd.spawn().map_err(|e| e.to_string())
}

struct CliLoginState<'a, F>
where
    F: Fn(LoginPhase),
{
    output: String,
    auth_link: Option<String>,
    url_regex: Regex,
    on_phase: &'a F,
    success_markers: &'a [&'a str],
    start: Instant,
    timeout: Duration,
}

impl<'a, F> CliLoginState<'a, F>
where
    F: Fn(LoginPhase),
{
    fn new(timeout_secs: u64, on_phase: &'a F, success_markers: &'a [&'a str]) -> Self {
        Self {
            output: String::new(),
            auth_link: None,
            url_regex: Regex::new(r"https?://[A-Za-z0-9._~:/?#\[\]@!$&'()*+,;=%-]+").unwrap(),
            on_phase,
            success_markers,
            start: Instant::now(),
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    fn handle_line(&mut self, line: &str) -> Option<LoginOutcome> {
        self.output.push_str(line);
        self.output.push('\n');
        self.capture_auth_link(line);

        if self
            .success_markers
            .iter()
            .any(|marker| line.contains(marker))
        {
            (self.on_phase)(LoginPhase::Complete);
            return Some(LoginOutcome::Success);
        }

        self.start
            .elapsed()
            .gt(&self.timeout)
            .then_some(LoginOutcome::TimedOut)
    }

    fn capture_auth_link(&mut self, line: &str) {
        if self.auth_link.is_some() {
            return;
        }

        let Some(m) = self.url_regex.find(line) else {
            return;
        };

        self.auth_link = Some(m.as_str().to_string());
        (self.on_phase)(LoginPhase::WaitingBrowser);
        let _ = open::that(m.as_str());
    }

    fn into_result(self, outcome: LoginOutcome) -> LoginResult {
        LoginResult {
            outcome,
            output: self.output,
            auth_link: self.auth_link,
        }
    }
}

fn spawn_login_reader<R>(stream: Option<R>, tx: Sender<String>)
where
    R: Read + Send + 'static,
{
    let Some(stream) = stream else {
        return;
    };
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
}

fn stop_child_with_outcome<F>(
    child: &mut Child,
    state: CliLoginState<'_, F>,
    outcome: LoginOutcome,
) -> LoginResult
where
    F: Fn(LoginPhase),
{
    let _ = child.kill();
    state.into_result(outcome)
}

fn wait_for_login_exit<F>(
    mut child: Child,
    state: CliLoginState<'_, F>,
    on_phase: &F,
) -> LoginResult
where
    F: Fn(LoginPhase),
{
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    on_phase(LoginPhase::Complete);
                    return state.into_result(LoginOutcome::Success);
                }
                return state.into_result(LoginOutcome::Failed {
                    status: status.code().unwrap_or(-1),
                });
            }
            Ok(None) if state.start.elapsed() > state.timeout => {
                let _ = child.kill();
                return state.into_result(LoginOutcome::TimedOut);
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(e) => return state.into_result(LoginOutcome::LaunchFailed(e.to_string())),
        }
    }
}

/// Open a URL in the default browser
pub fn open_auth_url(url: &str) -> anyhow::Result<()> {
    open::that(url)?;
    Ok(())
}
