/**
 * Provider "external surface" capabilities for the tray detail view.
 *
 * These sets state which providers expose an external dashboard URL and which
 * expose an external status page in the backend. They gate the contextual
 * action buttons (控制台 / 状态监控) in the detail card; a provider with no such
 * URL gets no button, never a broken stub.
 *
 * Moved out of TrayPanel so the card can render the gated buttons without a
 * circular import (TrayPanel renders TrayCard, TrayCard gates on these sets).
 */
export const HAS_DASHBOARD = new Set([
  "abacus", "alibaba", "alibabatokenplan", "amp", "augment",
  "azureopenai", "bedrock", "claude", "codex", "codebuff",
  "commandcode", "copilot", "crof", "crossmodel", "cursor", "deepgram", "deepseek",
  "doubao", "arkcodingplan", "arkagentplan", "elevenlabs", "factory", "gemini", "grok", "groq",
  "infini", "jetbrains", "kilo", "kimi", "kimik2", "kiro", "manus",
  "mimo", "mimoapi", "minimax", "mistral", "nanogpt", "ollama", "openaiapi",
  "opencode", "opencodego", "openrouter", "perplexity", "qoder", "sakana", "stepfun",
  "t3chat", "venice", "vertexai", "warp", "windsurf", "zai",
]);

export const HAS_STATUS_PAGE = new Set([
  "alibabatokenplan", "amp", "augment", "azureopenai", "bedrock",
  "claude", "codex", "copilot", "deepgram", "deepseek", "elevenlabs",
  "gemini", "grok", "groq", "kiro", "mistral", "openaiapi",
  "openrouter", "vertexai", "windsurf",
]);
