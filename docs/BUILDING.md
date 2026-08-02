# Building from Source

## Prerequisites

- **Rust** stable with the `x86_64-pc-windows-msvc` target
- **Microsoft Visual Studio Build Tools** with the **Desktop development with C++** workload
- **Node.js** 20+ and pnpm

Install the tools manually with rustup/winget/corepack, or use a tool manager
such as mise. There is no automatic Windows bootstrap script in this port.

## Build the Desktop App

```powershell
cd apps/desktop-tauri
pnpm install --frozen-lockfile
cd ../..
pnpm --dir apps/desktop-tauri run tauri:build
```

The release binary lands at `target/release/codexbar-desktop-tauri.exe`.

For a debug build (faster compile, no optimisations):
```powershell
cd apps/desktop-tauri
pnpm run tauri:build:debug
```

## Build the CLI Only

```powershell
cargo build -p codexbar --release
# Binary at: target/release/codexbar.exe
```

## Dev Mode (Hot Reload)

Use the Windows entry point from the repository root:

```powershell
.\scripts\dev-windows.ps1
```

For a no-console double-click launch, open `scripts\dev-windows.vbs`; it
forwards any arguments to the same PowerShell entry point with a hidden window.

It first removes only TokenBar processes associated with this checkout (the
debug Tauri executable, its Vite server, and their child processes), then
starts the complete `tauri dev` chain through pnpm. The child command runs with
a hidden window; stdout/stderr are captured in `%TEMP%\tokenbar-dev\` so a
second console or duplicate tray process is not created. Re-running the entry
point is therefore safe and deterministic.

The normal entry clears any inherited `CODEXBAR_PROOF_MODE`, so the flyout's
outside-click dismissal and tray-toggle close actions work normally. For an
automation screenshot run that must keep the flyout visible, opt in explicitly:

```powershell
.\scripts\dev-windows.ps1 -ProofMode trayPanel
```

Inspect the cleanup selection without stopping or starting anything:

```powershell
.\scripts\dev-windows.ps1 -DryRun
```

The equivalent package command is `pnpm --dir apps/desktop-tauri run
dev:windows`. Keep `scripts/dev.ps1` for the existing build-and-run workflow
when a standalone debug or release binary is required.

## Fast Windows Release Build

For repeat release builds on a Windows server, prefer the cached release script:

```powershell
.\scripts\windows-release-build.ps1 -Ref v0.27.4
```

It builds from a clean managed checkout but keeps Cargo output, the pnpm store,
and signed installer bootstrapper downloads in `C:\code\Win-CodexBar-release\cache`.
Release assets land in `C:\code\Win-CodexBar-release\assets`. Keep the
`.sha256` sidecars; they are the copy/paste source for Winget's
`InstallerSha256`.

Useful release flags:

```powershell
.\scripts\windows-release-build.ps1 -Ref v0.27.5 -WarmCacheOnly
.\scripts\windows-release-build.ps1 -Ref v0.27.5 -WarmCliCache
.\scripts\windows-release-build.ps1 -Ref v0.27.5 -SmokeInstall
.\scripts\windows-release-build.ps1 -Ref v0.27.5 -UploadRelease v0.27.5
.\scripts\release-doctor.ps1 -Version 0.27.5
```

There is no hosted CI/CD for this repository right now. Run local checks before
PRs; the Windows release script is the primary path for installer and portable
artifacts.

## macOS Windows Cross Build

For a fast compile check from macOS, use the cross-build wrapper:

```bash
./scripts/macos-windows-cross-build.sh
```

Or call the desktop package script directly:

```bash
pnpm --dir apps/desktop-tauri run tauri:build:windows-cross
```

This uses `cargo-xwin` plus Homebrew `llvm`/`lld` to build the Windows MSVC
Tauri executable at `target/x86_64-pc-windows-msvc/release/codexbar-desktop-tauri.exe`.
It is useful for catching frontend, Tauri, and Windows-target Rust compile
failures from a Mac. It does not replace the Windows server release path:
installer packaging, tray behavior, WebView2, DPAPI, startup integration, and
smoke install validation still need a real Windows machine.

## Project Structure

```
Win-CodexBar/
├── apps/desktop-tauri/          # Tauri desktop shell
│   ├── src/                     # React frontend (TypeScript)
│   └── src-tauri/               # Tauri/Rust backend
│       └── src/
│           ├── commands/        # Tauri IPC commands
│           ├── shell/           # Window management, DWM, tray bridge
│           └── main.rs          # App entry point
├── rust/                        # Shared backend crate + CLI
│   └── src/
│       ├── providers/           # Per-provider fetch/parse/auth
│       ├── core/                # Provider IDs, cost pricing
│       ├── browser/             # Browser cookie extraction (DPAPI)
│       ├── tray/                # Tray icon rendering
│       └── main.rs              # CLI entry point
├── docs/                        # Documentation
└── scripts/                     # Dev/release helper scripts
```

## Running Tests

```bash
# Shared crate tests
cargo test --manifest-path rust/Cargo.toml

# Tauri crate tests
cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml

# TypeScript type check
cd apps/desktop-tauri && pnpm exec tsc --noEmit

# Lint
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```
