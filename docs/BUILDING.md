# Building TokenBar on Windows

TokenBar is a Windows desktop application. The supported desktop build uses
the Tauri shell in `apps/desktop-tauri` and the shared Rust code in `rust/`.

## Prerequisites

- Windows 10 or Windows 11
- Node.js 20 or newer with pnpm
- Rust stable toolchain (including the MSVC target)
- Microsoft Edge WebView2 Runtime for running the desktop shell

## Development build

From the repository root:

```powershell
pnpm --dir apps/desktop-tauri install
pnpm --dir apps/desktop-tauri tauri:dev
```

The helper scripts in `scripts/` can be used when a repeatable Windows launch
or a CLI-only workflow is preferred.

## Checks and tests

```powershell
cargo test --manifest-path rust/Cargo.toml
cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml
pnpm --dir apps/desktop-tauri test
pnpm --dir apps/desktop-tauri run build
```

Run `cargo fmt --all` before submitting Rust changes.

## Release build

```powershell
pnpm --dir apps/desktop-tauri tauri:build
```

The release workflow produces the Tauri application bundle. Installer,
portable, and checksum assets are published from the GitHub release workflow.
Do not include credentials, browser profiles, generated build directories, or
local logs in a commit.
