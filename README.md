# TokenBar

> Every AI model quota, in your menu bar.

TokenBar is a native menu bar app that shows the **quota, usage, balance, reset windows, and availability** of AI coding / model platforms — Codex, OpenCode Go, MiniMax, MiMo, DeepSeek, Doubao / Volcengine Ark, Volc Engine Agent & coding count-based plans, and more.

- **Native first.** macOS menu bar app (Swift / SwiftUI). Windows in a later phase.
- **Multi-source.** API Key, browser cookie, local file, CLI config, snapshot — whatever the platform exposes.
- **Multi-window.** 5-hour / 7-day / weekly / monthly / balance / count / token / request-limit windows, each with its own reset countdown.
- **Honest data.** When a quota cannot be fetched, TokenBar shows `unknown` — it never fabricates numbers.
- **Privacy first.** Reuses existing local sessions (OAuth, cookies, CLI config). No passwords stored. Cookies are opt-in.

> **Status: scaffold phase.** This branch defines the product plan, architecture, and module scaffold only. No provider business logic is implemented yet. See [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md).

---

## Why

AI coding platforms each expose quota differently: some by API key, some by browser cookie, some only by local CLI config, some by count of requests, some by token spend, some by 5-hour rolling windows, some by weekly / monthly cycles. TokenBar puts all of them behind one menu bar icon with per-provider cards and reset countdowns, so you can plan long tasks around resets instead of guessing.

## Platforms (long-term scope)

| Platform | Primary source candidates |
| --- | --- |
| Codex (OpenAI) | OAuth API, local Codex CLI config |
| OpenCode Go | Browser cookie / local SQLite |
| MiniMax | API token / cookie header / browser cookie |
| Xiaomi MiMo | Browser cookie |
| DeepSeek | API key (credit balance) |
| Doubao / Volcengine Ark | API key (request-limit probe) |
| Volc Engine Agent / coding count plans | API key / cookie |
| ccswitch configs | Local CLI config import |

Additional platforms can be added via the provider adapter contract (see [docs/PROVIDER_SOURCE_STRATEGY.md](docs/PROVIDER_SOURCE_STRATEGY.md)).

## Architecture (summary)

TokenBar follows a clean **core / app / cli** separation, inspired by [CodexBar](https://github.com/steipete/CodexBar):

- `TokenBarCore` — provider adapters, usage windows, source readers, config & snapshot storage. No UI.
- `TokenBar` — app: menu bar status item, popover, provider cards, settings, state stores.
- `TokenBarCLI` — bundled CLI for scripts / CI.

Data flow: background refresh → provider probes → `UsageStore` → menu icon / popover / widgets.

See [docs/TOKENBAR_ARCHITECTURE.md](docs/TOKENBAR_ARCHITECTURE.md).

## Phases

- **Phase 0 — Scaffold (this branch).** Docs + module scaffold. No business logic.
- **Phase 1 — Mac MVP.** App shell, one end-to-end provider, menu bar + popover + manual refresh + settings + source status. See [docs/MAC_MVP_PLAN.md](docs/MAC_MVP_PLAN.md).
- **Phase 2 — Provider coverage.** All long-term providers, multi-window rendering, ccswitch import.
- **Phase 3 — Windows.** Native Windows port.

## Develop

Requires macOS 14+ and Swift 6.

```bash
swift build
swift test
```

## Attribution

TokenBar is an independent project. Its architecture is informed by [CodexBar](https://github.com/steipete/CodexBar) by Peter Steinberger (MIT). See [NOTICE](NOTICE).

## License

MIT — see [LICENSE](LICENSE).
