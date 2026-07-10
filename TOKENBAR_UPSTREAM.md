# Windows upstream baseline

`platform/windows` is based on [Finesssee/Win-CodexBar](https://github.com/Finesssee/Win-CodexBar), imported from commit `7b4b64b3e04153e26004f8620e8b657cebaa912a` on 2026-07-10.

The imported project is MIT licensed. Its `LICENSE`, notices, and attribution must remain intact in all downstream work.

This branch is the active Windows implementation line for TokenBar. It uses Tauri, React, and Rust, with a Windows system-tray panel and shared provider backend. Before enabling any provider for personal use, review its credential and browser-cookie behavior; cookie import must remain opt-in.

The macOS Swift prototype remains isolated in `platform/macos`. `main` contains only the active branch map and no product code.
