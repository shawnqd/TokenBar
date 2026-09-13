#Requires -Version 5.1
<#
.SYNOPSIS
    Start one hidden Windows Tauri development chain for TokenBar.

.DESCRIPTION
    Stops only processes that can be attributed to this checkout (the debug
    Tauri binary, its Vite server, and their descendants), then starts
    `tauri dev` through pnpm. Child process output is written to a temporary
    log so no extra console window is created. Normal launches clear
    `CODEXBAR_PROOF_MODE`; pass `-ProofMode trayPanel` only for screenshot
    automation that intentionally needs blur-dismiss suppressed.

    Use -DryRun to inspect the processes that would be stopped without
    changing process state or starting Tauri.
#>

param(
    [switch]$DryRun,
    [switch]$StopOnly,
    [switch]$Verbose,
    [switch]$StartVisible,
    [string]$ProofMode
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path.TrimEnd("\")
$FrontendDir = Join-Path $RepoRoot "apps\desktop-tauri"
$TargetDir = Join-Path $RepoRoot "target"
$TargetPrefix = "$TargetDir\"
$FrontendPrefix = "$FrontendDir\"

function Test-RepoPath {
    param([AllowNull()][string]$Path, [string]$Prefix)
    if ([string]::IsNullOrWhiteSpace($Path)) { return $false }
    return $Path.StartsWith($Prefix, [System.StringComparison]::OrdinalIgnoreCase)
}

function Test-RepoReference {
    param([AllowNull()][string]$Text, [string]$Reference)
    if ([string]::IsNullOrWhiteSpace($Text)) { return $false }
    return $Text.IndexOf($Reference, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
}

function Get-TokenBarProcesses {
    $all = @(Get-CimInstance Win32_Process)
    $byId = @{}
    foreach ($process in $all) {
        $byId[[int]$process.ProcessId] = $process
    }

    # Roots are identified by executable path or command line containing this
    # checkout. Descendants are included only after a root has been identified.
    $roots = @($all | Where-Object {
        $exePath = [string]$_.ExecutablePath
        $commandLine = [string]$_.CommandLine
        $name = [string]$_.Name
        $isDesktopBinary = $name -ieq "codexbar-desktop-tauri.exe" -and
            (Test-RepoPath $exePath $TargetPrefix)
        $isFrontendVite = $name -ieq "node.exe" -and
            (Test-RepoReference $commandLine $FrontendPrefix) -and
            ($commandLine -match "vite(?:\\|/)bin(?:\\|/)vite\.js")
        $isTauriDev = ($commandLine -match '(?i)(tauri(?:\.js|\.cjs)?|@tauri-apps[\\/].*tauri)') -and
            ($commandLine -match '(?i)(^|\s)dev(\s|$)') -and
            (($commandLine -match [regex]::Escape($FrontendDir)) -or
             ($commandLine -match '(?i)--dir\s+.*apps[\\/]desktop-tauri'))
        $isPnpmDev = ($commandLine -match '(?i)pnpm') -and
            ($commandLine -match '(?i)tauri:dev|exec\s+tauri\s+dev') -and
            (($commandLine -match [regex]::Escape($FrontendDir)) -or
             ($commandLine -match '(?i)--dir\s+.*apps[\\/]desktop-tauri'))
        $isDesktopBinary -or $isFrontendVite -or $isTauriDev -or $isPnpmDev
    })

    # Never walk an unrestricted process tree here. A Tauri/WebView process can
    # be re-parented by Windows, and following that relationship can reach
    # unrelated system services. Only carry the checkout-owned process names
    # below, and require either a TokenBar command line or the app's private
    # WebView user-data directory.
    $allowedChildNames = @(
        "codexbar-desktop-tauri.exe",
        "node.exe",
        "cmd.exe",
        "cargo.exe",
        "rustc.exe",
        "esbuild.exe",
        "conhost.exe",
        "msedgewebview2.exe"
    )

    $ids = [System.Collections.Generic.HashSet[int]]::new()
    foreach ($root in $roots) {
        [void]$ids.Add([int]$root.ProcessId)
    }

    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($process in $all) {
            $processId = [int]$process.ProcessId
            if ($ids.Contains($processId) -or -not $ids.Contains([int]$process.ParentProcessId)) {
                continue
            }

            $name = [string]$process.Name
            $commandLine = [string]$process.CommandLine
            if ($allowedChildNames -notcontains $name) {
                continue
            }

            $isRepoCommand = Test-RepoReference $commandLine $RepoRoot
            $isTokenBarWebView = $name -ieq "msedgewebview2.exe" -and
                (Test-RepoReference $commandLine "--webview-exe-name=codexbar-desktop-tauri.exe")
            if (-not ($isRepoCommand -or $isTokenBarWebView)) {
                continue
            }

            [void]$ids.Add($processId)
            $changed = $true
        }
    }

    return @($ids | ForEach-Object { $byId[[int]$_] } | Where-Object { $null -ne $_ } |
        Sort-Object ProcessId)
}

function Stop-TokenBarProcesses {
    $processes = @(Get-TokenBarProcesses)
    if ($processes.Count -eq 0) {
        Write-Host "No existing TokenBar development processes found." -ForegroundColor DarkGray
        return
    }

    Write-Host "TokenBar development processes selected for cleanup:" -ForegroundColor Yellow
    foreach ($process in $processes) {
        Write-Host ("  PID {0,-6} {1}  {2}" -f $process.ProcessId, $process.Name, ([string]$process.CommandLine).Trim())
    }
    if ($DryRun) { return }

    # Stop children first, then roots. No process-name wildcard or broad
    # Stop-Process is used; every PID came from the checkout-scoped matcher.
    foreach ($process in ($processes | Sort-Object @{Expression = { $_.ParentProcessId }; Descending = $true })) {
        Stop-Process -Id ([int]$process.ProcessId) -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 250
}

if (-not (Test-Path -LiteralPath $FrontendDir)) {
    throw "Missing desktop app directory: $FrontendDir"
}

if (-not $DryRun -and -not $StopOnly) {
    $lockPath = Join-Path $env:TEMP "tokenbar-dev.lock"
    try {
        # An exclusive handle closes automatically if the launcher is
        # interrupted; unlike a PID file, it cannot leave a permanent stale
        # lock behind.
        $devLock = [System.IO.File]::Open(
            $lockPath,
            [System.IO.FileMode]::OpenOrCreate,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
    } catch {
        throw "Another TokenBar Windows development launcher is already running."
    }
}

Stop-TokenBarProcesses
if ($DryRun) {
    Write-Host "Dry-run complete; no processes were stopped and Tauri was not started." -ForegroundColor Cyan
    exit 0
}
if ($StopOnly) {
    Write-Host "Cleanup complete; Tauri was not started." -ForegroundColor Cyan
    exit 0
}

# This launcher is the normal interactive development entry point. Proof mode
# is intentionally sticky in the process environment because it is used by
# screenshot/automation harnesses; inheriting that variable here would make a
# user-facing dev build suppress blur-dismiss and tray-toggle close actions.
# Keep proof runs explicit and separate from the normal launcher.
Remove-Item Env:CODEXBAR_PROOF_MODE -ErrorAction SilentlyContinue
if ($StartVisible) {
    # Normal product launches are tray-first. This opt-in is for a human
    # verification run that must show the real tray panel immediately after
    # the single development chain starts; it does not change persisted
    # settings or the installed app's default behaviour.
    $env:CODEXBAR_START_VISIBLE = "1"
} else {
    # Do not let a stale shell variable silently turn a normal launch into a
    # visible/proof run. The mode must be explicit at the launcher boundary.
    Remove-Item Env:CODEXBAR_START_VISIBLE -ErrorAction SilentlyContinue
}
if (-not [string]::IsNullOrWhiteSpace($ProofMode)) {
    $env:CODEXBAR_PROOF_MODE = $ProofMode.Trim()
}

$pnpm = Get-Command pnpm.cmd -ErrorAction SilentlyContinue
if (-not $pnpm) { throw "pnpm.cmd was not found. Install pnpm before starting TokenBar development." }

$logDir = Join-Path $env:TEMP "tokenbar-dev"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$stdoutLog = Join-Path $logDir "tauri-dev.stdout.log"
$stderrLog = Join-Path $logDir "tauri-dev.stderr.log"

$arguments = @("--dir", ('"{0}"' -f $FrontendDir), "run", "tauri:dev")

# Windows PowerShell 5.1 builds the child environment as a case-insensitive
# dictionary and throws "已添加项。字典中的关键字…" when the parent carries the same
# name in two casings (http_proxy + HTTP_PROXY). Proxy clients and agent
# sandboxes both produce that pair, and the launcher is otherwise unusable on
# those machines. Collapse each pair to a single lowercase entry — the spelling
# curl/Go/reqwest read first — before spawning, so the child still sees a proxy
# but the dictionary stays unique.
foreach ($proxyName in @("http_proxy", "https_proxy", "all_proxy", "no_proxy")) {
    $upperName = $proxyName.ToUpperInvariant()
    $lowerValue = [System.Environment]::GetEnvironmentVariable($proxyName)
    $upperValue = [System.Environment]::GetEnvironmentVariable($upperName)
    if ($null -eq $lowerValue -and $null -eq $upperValue) { continue }
    if ($null -eq $lowerValue) { $lowerValue = $upperValue }
    [System.Environment]::SetEnvironmentVariable($upperName, $null)
    [System.Environment]::SetEnvironmentVariable($proxyName, $null)
    [System.Environment]::SetEnvironmentVariable($proxyName, $lowerValue)
}

if ($StartVisible) {
    Write-Host "Starting one visible Tauri development chain (tray panel)..." -ForegroundColor Green
} else {
    Write-Host "Starting one tray-first Tauri development chain..." -ForegroundColor Green
}
Write-Host "Logs: $stdoutLog and $stderrLog" -ForegroundColor DarkGray

$process = Start-Process -FilePath $pnpm.Source `
    -ArgumentList $arguments `
    -WorkingDirectory $RepoRoot `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdoutLog `
    -RedirectStandardError $stderrLog `
    -PassThru

if ($Verbose) {
    Write-Host "Tauri dev PID: $($process.Id)" -ForegroundColor DarkGray
}

$process.WaitForExit()
$exitCode = $process.ExitCode
$process.Dispose()
$devLock.Dispose()

if ($exitCode -ne 0) {
    Write-Host "Tauri development chain exited with code $exitCode. See the logs above." -ForegroundColor Red
    exit $exitCode
}
