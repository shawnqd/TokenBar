//! Startup staging: one subsystem failing must not take the app with it.
//!
//! # Why
//!
//! The `setup` closure brings up eight or nine independent things — the tray
//! icon, two prewarmed WebViews, the taskbar strip, the global shortcut, the
//! floating bar, auto-refresh, the chart cache. They are independent in the
//! sense that matters to a user: with no floating bar you still have a tray
//! icon; with no global shortcut everything else still works.
//!
//! They were not independent in the sense that matters to the process. Two used
//! `?`, so a failure ended `setup` and the app exited. The rest were called
//! bare, so a panic anywhere inside them unwound into Tauri's event-loop
//! callback — an `extern` frame — where unwinding is not allowed and the
//! runtime aborts.
//!
//! That is not hypothetical. A single missing escape in a `.ftl` file made the
//! Fluent bundle fail to parse; the first string lookup panicked; it happened
//! inside the Settings prewarm, which *was* guarded with `if let Err(..)`. The
//! guard did nothing, because a panic is not an `Err`. The whole app aborted at
//! launch over one line of translation data.
//!
//! # What this does
//!
//! [`stage`] runs one subsystem's initialiser inside `catch_unwind` and logs
//! whatever comes out. A stage that fails is reported and skipped; the ones
//! after it still run.
//!
//! # What this deliberately does not do
//!
//! It does not make anything *recoverable*. A failed stage stays failed for the
//! session — there is no retry and no degraded mode beyond "that feature is
//! absent". The goal is only that the absence is confined to that feature.
//!
//! And it is not a licence to panic. A panic reaching here is still a bug and
//! still gets a loud log line with the stage's name; this only decides what it
//! costs. Stages that can fail for ordinary reasons should still return `Err`
//! and get a plain warning rather than a caught panic.

use std::panic::{AssertUnwindSafe, catch_unwind};

/// Run one startup stage, containing both `Err` and panic.
///
/// `name` appears in every log line this produces, so it should read as the
/// feature the user loses — "tray icon", "taskbar strip" — rather than as the
/// function being called.
pub fn stage<T, E>(name: &str, init: impl FnOnce() -> Result<T, E>)
where
    E: std::fmt::Display,
{
    // `AssertUnwindSafe` because the closures capture `&AppHandle` and Tauri's
    // handles are not `UnwindSafe`. The assertion is sound here in the way that
    // matters: a panicking stage leaves *its own* feature half-built, and the
    // stage is then abandoned for the session rather than resumed, so nothing
    // reads back state a panic left inconsistent.
    match catch_unwind(AssertUnwindSafe(init)) {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            tracing::warn!(stage = name, %error, "startup: stage failed; continuing without it");
        }
        Err(payload) => {
            tracing::error!(
                stage = name,
                panic = %panic_message(&payload),
                "startup: stage PANICKED; continuing without it — this is a bug, \
                 not a supported degraded mode"
            );
        }
    }
}

/// Same, for a stage with nothing to report but its own panics.
pub fn stage_infallible(name: &str, init: impl FnOnce()) {
    stage(name, || {
        init();
        Ok::<(), std::convert::Infallible>(())
    });
}

/// Best-effort text out of a panic payload. `panic!` produces a `&str` for a
/// literal and a `String` once formatted; anything else is opaque.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_stage_does_not_propagate() {
        // The whole point: this test finishing at all is the assertion.
        stage_infallible("deliberately broken", || panic!("boom"));
        stage("also broken", || Err::<(), _>("no"));
    }

    #[test]
    fn later_stages_still_run_after_an_earlier_one_dies() {
        let mut ran = false;
        stage_infallible("first", || panic!("boom"));
        stage_infallible("second", || ran = true);
        assert!(ran, "a failed stage must not skip the ones after it");
    }

    #[test]
    fn panic_messages_survive_for_the_log() {
        let literal: Box<dyn std::any::Any + Send> = Box::new("literal");
        let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("owned"));
        let opaque: Box<dyn std::any::Any + Send> = Box::new(7u8);
        assert_eq!(panic_message(&literal), "literal");
        assert_eq!(panic_message(&owned), "owned");
        assert_eq!(panic_message(&opaque), "<non-string panic payload>");
    }
}
