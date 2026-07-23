# Repository Guidelines

## 跨终端交接文档（必须先读）

本项目采用跨终端开发交接规范（Codex / Claude Code / OpenCode / Grok 等）。任何模型进入本仓库前，按以下顺序读取实际存在的文件：

1. `README.md`
2. [COLLABORATION.md](COLLABORATION.md) — 单一写入者、分支边界、跨工具协作规则
3. [CURRENT_TASK.md](CURRENT_TASK.md) — 当前唯一任务包
4. [AGENT_HANDOFF.md](AGENT_HANDOFF.md) — 唯一当前交接入口（最新 checkpoint 在顶部）
5. [PROJECT_STATUS.md](PROJECT_STATUS.md) — 项目阶段、已完成、阻塞项
6. 任务点名的领域文档（本文件其余部分、`docs/PROJECT_LINES.md` 等）
7. [DECISIONS.md](DECISIONS.md) — 涉及产品、架构、安全或破坏性操作时必读
8. 最近相关的 [CHANGELOG.md](CHANGELOG.md)、[CODE_REVIEW.md](CODE_REVIEW.md)

`logs/dev_audit.jsonl` 是按行追加的执行审计日志；当前开发平台/模型在每轮 checkpoint 时负责追加，无法确认的字段写 `unknown`，不得猜测。

## Current Project State
- This branch launches the Tauri desktop shell by default (`apps/desktop-tauri/src-tauri`), while
  `rust/` remains the shared backend/domain crate and standalone CLI.
- Many files in `docs/` and some workflows reference the upstream macOS/Swift project. Treat those as historical or
  upstream-sync material unless the task is explicitly about upstream parity.
- When repo docs conflict, trust the active Tauri desktop sources in `apps/desktop-tauri` plus the shared Rust sources
  in `rust/src`.

## Project Structure & Modules
- `apps/desktop-tauri/`: Tauri desktop shell (default UI). React frontend in `apps/desktop-tauri/src/`,
  Rust backend + tray bridge in `apps/desktop-tauri/src-tauri/src/`.
- `rust/src`: Shared backend crate + CLI (`codexbar` binary). Houses providers, settings, login,
  status, sound, shortcuts, browser cookie extraction, and the shared tray-icon renderer.
- `rust/src/providers`: Provider-specific fetch/parsing/auth logic. Keep provider boundaries clean.
- `rust/src/tray` (shared): `icon.rs` + `render.rs` — pixel-level tray-icon rendering used by the Tauri shell.
- `rust/src/browser`: Browser detection + cookie extraction for Windows.
- `rust/src/core`: Shared provider-construction (`instantiate_provider`) and provider IDs.
- `rust/assets`, `rust/icons`, `rust/gen`, `rust/wix`: UI assets, generated schemas, installer packaging.
- `docs`: Mixed documentation (Windows port docs plus upstream/macOS references). Update only the relevant docs.

## Build, Test, Run
- Default desktop work runs from the repo root; `cd rust` is for backend/CLI-only tasks.
- Build the desktop shell (preferred): `cd apps/desktop-tauri && npm run tauri:build` (or `tauri:build:debug`).
  Raw `cargo build --release` on the Tauri crate produces an exe that still points at the dev URL.
- Build the CLI: `cargo build -p codexbar`.
- Test: `cargo test --manifest-path rust/Cargo.toml` and
  `cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml`.
- Run CLI locally: `cargo run -p codexbar -- --help`, `cargo run -p codexbar -- usage -p claude`,
  `cargo run -p codexbar -- cost`. The CLI no longer launches a GUI when run with no subcommand.
- Run the desktop shell through Tauri's build/dev flow: `.\dev.ps1`, `./dev.sh`, or
  `cd apps/desktop-tauri && npm run tauri:dev`.
- Format/lint before handoff when code changed: `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings`
  on both manifests (or explain why not run).
- There is no active root-level `Scripts/` build pipeline in this port. Do not rely on legacy `Scripts/*.sh` commands.

## Coding Style & Naming
- Prefer small, typed structs/enums and focused modules; keep changes local.
- Keep provider-specific logic inside the provider module instead of adding cross-provider branching.
- Preserve clear error handling and user-facing diagnostics (`anyhow`/`thiserror` + friendly messages where applicable).
- Use `tracing` for diagnostics; do not log raw secrets, cookies, or tokens.
- Avoid adding dependencies/tooling without confirmation.

## Testing Guidelines
- Add or extend focused Rust tests near the changed module (`#[cfg(test)]` unit tests are common in this repo).
- For parser/fetcher changes, add deterministic samples/fixtures where practical.
- Run `cargo test` after code changes; include any skipped checks in handoff.
- If desktop/tray behavior changed, do a manual validation with the Tauri shell when possible (`cargo run` or
  `codexbar-desktop-tauri`).

## Commit & PR Guidelines
- Use short imperative commit messages (for example: `Fix Claude CLI parser`, `Improve cookie import errors`).
- Keep commits scoped to one change.
- In PRs/patches, include:
  - Summary of behavior changes
  - Commands run (`cargo test`, `cargo fmt`, etc.)
  - Screenshots/GIFs for UI changes (Windows)
  - Linked issue/reference when relevant

## Release & Winget Notes
- Treat Winget updates as a normal release step after GitHub release artifacts are stable.
- Winget does not track "latest" GitHub releases; every version needs its own immutable manifest folder in
  `microsoft/winget-pkgs`, for example `manifests/f/Finesssee/Win-CodexBar/0.23.6/`.
- For routine version bumps, copy the previous approved manifest folder and change only version-specific fields:
  `PackageVersion`, `InstallerUrl`, `InstallerSha256`, `DisplayName`, `DisplayVersion`, `ReleaseNotes`, and
  `ReleaseNotesUrl`.
- Keep stable package identity and installer behavior unchanged unless there is a real packaging reason:
  `PackageIdentifier`, `InstallerType`, `Scope`, `ProductCode`, `Publisher`, package URLs, and silent install behavior.
- Before opening a Winget PR, verify the release installer URL resolves and recompute the SHA-256 from the downloaded
  asset. On Windows, run `winget validate` when available.
- The first Winget package submission was approved in `microsoft/winget-pkgs#366653`; the v0.23.5 update was approved
  in `microsoft/winget-pkgs#366794`. Future updates should be faster, but still expect Microsoft validation/review.

## Agent Notes
- The default desktop app is the Tauri shell in `apps/desktop-tauri/`. The Rust crate owns shared backend logic
  and the CLI.
- New provider construction goes through `codexbar::core::instantiate_provider` — do not duplicate provider
  factories in shells or commands.
- Keep provider data siloed: never show identity/plan/email fields from provider A in provider B UI.
- Claude CLI output is user-configurable; do not depend on a customizable status line for usage parsing.
- Cookie import UX uses explicit browser selection in Preferences. Do not assume Chrome-only in general UI flows.
- Be conservative with secret handling (manual cookies, API keys, token accounts); use existing redaction/storage helpers.
- Prefer Windows-native validation for tray/DPAPI/browser-cookie behavior; WSL/Linux can be insufficient for those paths.
