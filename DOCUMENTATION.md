# Project Documentation Map

TokenBar follows the current `cross-tool-dev-workflow` contract from the
private `shawnqd/ai-skills-vault` repository. These files are the project's
shared language; tool-local files under `.claude/`, `.opencode/`, or `.codex/`
are configuration inputs only and never replace these documents.

## Stable project documents

| File | Responsibility |
| --- | --- |
| `AGENTS.md` | Repository rules, commands, scope and safety boundaries |
| `README.md` | Product entry point and long-term project boundaries |
| `COLLABORATION.md` | TokenBar's platform branches and cross-tool roles |
| `CURRENT_TASK.md` | The one active task contract; rewrite at task changes |
| `AGENT_HANDOFF.md` | Current task checkpoints, blockers and next action |
| `PROJECT_STATUS.md` | Project phase, completed work, blockers and next milestone |
| `CHANGELOG.md` | User-visible or formal engineering changes |
| `CODE_REVIEW.md` | Current findings, verification gaps and Go/No-Go |
| `DECISIONS.md` | Durable product, architecture, security and destructive-action decisions |

## Activity and machine logs

`PLATFORM_ACTIVITY_LOG.md` is the only human-readable platform activity
history. Each completed, partial or blocked task adds one concise factual row.
`logs/dev_audit.jsonl` is optional machine detail and must never replace the
activity log or contain secrets.

## Reading order

Before writing: `AGENTS.md` → `README.md` → `CURRENT_TASK.md` →
`AGENT_HANDOFF.md`.

On first entry or platform switch, also read `COLLABORATION.md` and
`PROJECT_STATUS.md`. Read `DECISIONS.md`, domain documents, `CODE_REVIEW.md`,
`PLATFORM_ACTIVITY_LOG.md`, or `CHANGELOG.md` only when the task requires them.

Historical material is kept under `docs/archive/` for traceability and is not
part of the current task contract.
