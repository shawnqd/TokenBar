# TokenBar

TokenBar is a Windows system-tray app for keeping AI coding-tool usage visible
without opening a separate dashboard for every provider. It brings the
provider coverage and usage-focused workflow of
[CodexBar](https://github.com/steipete/CodexBar) to a native Tauri + React
desktop shell with shared Rust logic.

[English](./README.md) | [简体中文](./README.zh-CN.md) | [繁體中文（臺灣）](./README.zh-TW.md) | [日本語](./README.ja-JP.md) | [한국어](./README.ko-KR.md) | [Español mexicano](./README.es-MX.md)

<p align="center">
  <img src="docs/images/tray-panel.png" width="320" alt="TokenBar tray panel"/>
  <img src="docs/images/settings-providers.png" width="520" alt="TokenBar provider settings"/>
</p>

## Features

- System-tray panel with compact provider cards and one-click refresh.
- Provider settings for credentials, session cookies, API keys, regions, and
  display preferences where a provider supports them.
- Automatic browser-cookie discovery and manual cookie import as separate
  choices; no application-owned login browser is required.
- A local CLI for usage, cost, configuration, and diagnostics.
- Windows installer and portable builds with SHA-256 checksums.
- English, Simplified Chinese, Traditional Chinese, Japanese, Korean, and
  Mexican Spanish UI translations.

## Installation

Download the latest Windows installer or portable build from
[TokenBar Releases](https://github.com/shawnqd/TokenBar/releases). Each release
contains the published executables and checksum sidecars when available.

TokenBar is currently distributed from GitHub Releases; a Winget package is not
enabled yet.

## First run

1. Start TokenBar from the Start menu or the portable executable.
2. Click the tray icon to open the usage panel.
3. Open **Settings → Providers** and enable the providers you use.
4. Configure the authentication source supported by each provider. Automatic
   browser reading and saved/manual cookies are different choices; API keys and
   provider CLIs are used only where supported.

## Build from source

Requirements: Windows 10/11, Node.js with pnpm, and a Rust toolchain.

```powershell
git clone https://github.com/shawnqd/TokenBar.git
cd TokenBar
pnpm --dir apps/desktop-tauri install
pnpm --dir apps/desktop-tauri tauri:dev
```

For a production build:

```powershell
pnpm --dir apps/desktop-tauri tauri:build
```

The helper scripts under `scripts/` provide Windows-specific development and
release workflows. See [docs/BUILDING.md](docs/BUILDING.md) for the supported
commands.

## Privacy

- Provider data is read from local configuration or APIs that you configure.
- Browser-cookie extraction runs only for enabled providers and never exposes
  raw cookies in diagnostics.
- API keys, manual cookies, and account data are stored through the protected
  credential layer available on Windows.
- Diagnostics contain provider/source/status metadata, not credentials.

## Documentation

- [Building from source](docs/BUILDING.md)
- [Browser cookies](docs/COOKIES.md)
- [WSL notes](docs/WSL.md)

## macOS

TokenBar is maintained for Windows. For macOS, use the original CodexBar:
[steipete/CodexBar](https://github.com/steipete/CodexBar).

## Credits and license

TokenBar is released under the MIT license. It builds on the ideas and provider
work of [CodexBar](https://github.com/steipete/CodexBar) and
[ccusage](https://github.com/ryoppippi/ccusage).
