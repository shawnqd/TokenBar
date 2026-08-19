export type SurfaceMode = "hidden" | "trayPanel" | "popOut" | "settings";
export type VisibleSurfaceMode = Exclude<SurfaceMode, "hidden">;
export type SettingsTabId =
  | "general"
  | "providers"
  | "notifications"
  | "menuBar"
  | "dashboard"
  | "floatBar"
  | "menu"
  | "trayPanel"
  | "taskbarStatus"
  | "appearance"
  | "privacy"
  | "advanced"
  | "about";

// ── Narrowed string-literal unions (persisted settings enums) ─────────

export type TrayIconMode = "single" | "perProvider";

export type MetricPreference =
  | "automatic"
  | "session"
  | "weekly"
  | "model"
  | "tertiary"
  | "credits"
  | "extraUsage"
  | "average";

export type Language =
  | "english"
  | "chinese"
  | "chinesetraditional"
  | "japanese"
  | "korean"
  | "spanish";

/** Language catalog entry from the Rust backend. */
export type LanguageOption = {
  /** Stable bridge/settings value (e.g. "english") */
  value: Language;
  /** Native display name (e.g. "English", "中文", "Español") */
  display: string;
};

export type UpdateChannel = "stable" | "beta";

export type ThemePreference = "auto" | "light" | "dark";

export type MenuBarDisplayMode = "minimal" | "compact" | "detailed";
/** Which period the panel's local-usage stats lead with. */
export type LocalUsagePeriod = "today" | "7d" | "30d";
export type FloatBarOrientation = "horizontal" | "vertical";
export type TaskbarWidgetPosition = "notification" | "left";
/** Numeric Win32/OpenType weight class (100 thin through 900 black). */
/**
 * OpenType `wght` axis value, 100..=1000.
 *
 * Continuous on a variable font: the native strip renders through DirectWrite's
 * `SetFontAxisValues`, which produces distinct strokes for values between the
 * named stops. On a static family DirectWrite still resolves to the nearest
 * installed face — `TaskbarFontFamily.variableWeight` says which is which.
 */
export type TaskbarWidgetFontWeight = number;

/**
 * Which quota window a taskbar entry refers to.
 *
 * The four named cycles are identified from the window's declared length when
 * it has one, and otherwise from the provider's own name for the slot, so a
 * provider that reports a percentage without a length is still identified.
 *
 * `primary` means "this provider's main quota, whatever cycle that is". It is
 * the fallback for a window neither source can name, and the backend offers it
 * only in that case — when a cycle can be named, the named kind is offered
 * instead so the menu never lists two options for one reading.
 */
export type TaskbarWindowKind =
  | "primary"
  | "session"
  | "weekly"
  | "daily"
  | "monthly"
  | "balance"
  | "speed";

/** `auto` follows whichever provider the tray icon is currently showing. */
export const TASKBAR_PROVIDER_AUTO = "auto";

/** One line of the taskbar strip: a provider plus one of its quota windows. */
export interface TaskbarEntry {
  providerId: string;
  window: TaskbarWindowKind;
}

/**
 * One cell of the taskbar strip, exactly as the renderer is painting it.
 *
 * Produced by the native renderer's own line buffer, not rebuilt here — the
 * settings preview shows these verbatim so it cannot drift from the strip.
 */
/**
 * Which cycle a quota window is.
 *
 * Decided once in Rust when the snapshot is built (`src-tauri/src/quota_cycle.rs`)
 * from the declared length and, when there is none, the provider's own name for
 * the slot. Read it — do not re-derive it from `windowMinutes`. Three surfaces
 * used to do exactly that and disagreed with each other.
 *
 * `null` for a window that is not a dated cycle ("Credits", "Balance") or whose
 * cycle fits no named band (a fortnight). Render those without a cycle word
 * rather than rounding to the nearest one.
 */
export type QuotaCycleKind = "session" | "daily" | "weekly" | "monthly";

export interface TaskbarPreviewLine {
  /** The provider's brand mark, or `null` when `text` carries its name. */
  glyph: string | null;
  /** The mark's colour as `#rrggbb`. Colour is what identifies the provider. */
  color: string | null;
  text: string;
}

/**
 * Per-cell badge metadata the native taskbar strip reports, so the settings
 * preview can draw the official brand SVG from the same registry the strip
 * rasterises (`providerIcons.ts`) instead of hunting by colour.
 */
export interface TaskbarStripIcon {
  providerId: string;
  /** Registry key — matches the `ProviderIcon-*.svg` / `providerIcons.ts` entry. */
  assetId: string;
  /** The mark's colour as `#rrggbb`. */
  brandColor: string;
  /** Single-character fallback when no SVG exists; `null` on the main path. */
  fallbackGlyph: string | null;
}

/**
 * One cell of the taskbar strip, exactly as the native renderer is painting it.
 *
 * `get_taskbar_preview_lines` returns the visible 0-4 cells in user-entry order
 * so the settings preview shows the strip verbatim and cannot drift. The cell is
 * a brand mark + short tag + value kept as a left-packed cluster, with a
 * lifecycle `state` that tells the preview how to treat a node that is not a
 * simple live percentage.
 */
export interface TaskbarStripCell {
  providerId: string;
  window: TaskbarWindowKind;
  /** Short cycle / balance label, already localised (e.g. "5h" / "周" / "余"). */
  tag: string;
  /** Displayable value ("82%" / "¥38" / "48.2 t/s" / "—" / reason). */
  value: string;
  state:
    | "ready"
    | "loading"
    | "refreshing"
    | "stale"
    | "error"
    | "notConfigured"
    | "unsupported"
    | "unknown";
  /** Readable reason for error / unsupported / notConfigured cells. */
  reason: string | null;
  /** Official badge metadata, or `null` when the cell carries no icon. */
  icon: TaskbarStripIcon | null;
  /** Legacy fields, kept so older callers and fixtures keep compiling. */
  glyph: string | null;
  color: string | null;
  text: string;
}

/** An installed font family the taskbar strip can use. */
export interface TaskbarFontFamily {
  name: string;
  /** True when the family exposes a real `wght` axis. */
  variableWeight: boolean;
  /** True when the family draws Chinese itself instead of falling back. */
  hasCjk: boolean;
  /** True for the short curated list the picker shows before "show all". */
  recommended: boolean;
}
/**
 * Which quota window the floating bar prints a reset for.
 *
 * `primary` is "whatever this provider leads with" and is what the bar showed
 * before the setting existed; the named windows are matched by the window's
 * declared length, so "weekly" means the same thing for every provider.
 */
export type FloatBarResetWindow =
  | "primary"
  | "session"
  | "weekly"
  | "daily"
  | "monthly";

/** Mirrors `FLOAT_BAR_MAX_RESET_WINDOWS` in shared Rust. */
export const FLOAT_BAR_MAX_RESET_WINDOWS = 3;

export type TaskbarWidgetContent = "usage" | "speed" | "usage_speed";
export type TaskbarWidgetTextAlign = "left" | "center" | "right";
export type FloatBarStyle = "floating" | "taskbar";
export type ProofProviderId =
  | "codex"
  | "claude"
  | "cursor"
  | "factory"
  | "gemini"
  | "antigravity"
  | "copilot"
  | "zai"
  | "minimax"
  | "kiro"
  | "vertexai"
  | "augment"
  | "opencode"
  | "kimi"
  | "kimik2"
  | "amp"
  | "warp"
  | "ollama"
  | "azureopenai"
  | "t3chat"
  | "openrouter"
  | "jetbrains"
  | "alibaba"
  | "alibabatokenplan"
  | "nanogpt"
  | "infini"
  | "perplexity"
  | "abacus"
  | "opencodego"
  | "kilo"
  | "bedrock"
  | "mistral"
  | "codebuff"
  | "deepseek"
  | "windsurf"
  | "manus"
  | "mimo"
  | "mimoapi"
  | "doubao"
  | "arkcodingplan"
  | "arkagentplan"
  | "commandcode"
  | "crof"
  | "stepfun"
  | "venice"
  | "openaiapi"
  | "grok"
  | "elevenlabs"
  | "deepgram"
  | "groq"
  | "llmproxy"
  | "chutes"
  | "litellm"
  | "poe"
  | "devin"
  | "zed"
  | "crossmodel"
  | "qoder"
  | "sakana"
  | "sub2api"
  | "wayfinder";

export type TrayPanelSurfaceTarget = { kind: "summary" };
export type PopOutSurfaceTarget =
  | { kind: "dashboard" }
  | { kind: "provider"; providerId: string };
export type SettingsSurfaceTarget = { kind: "settings"; tab: SettingsTabId };

export type SurfaceTarget =
  | TrayPanelSurfaceTarget
  | PopOutSurfaceTarget
  | SettingsSurfaceTarget;

export type SurfaceTargetForMode<M extends VisibleSurfaceMode> =
  M extends "trayPanel"
    ? TrayPanelSurfaceTarget
    : M extends "popOut"
      ? PopOutSurfaceTarget
      : SettingsSurfaceTarget;

export interface CurrentSurfaceState {
  mode: SurfaceMode;
  target: SurfaceTarget;
}

export interface ProofRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ProofStatePayload {
  mode: SurfaceMode;
  target: SurfaceTarget;
  windowRect: ProofRect | null;
  trayAnchor: ProofRect | null;
  workArea: ProofRect | null;
  menuPath: string | null;
  menuItems: string[];
}

export type ProofCommand =
  | "open-tray-panel"
  | "open-native-menu"
  | "open-dashboard"
  | "open-about-path"
  | "hide-surface"
  | `open-provider:${ProofProviderId}`
  | `open-settings:${SettingsTabId}`;

export interface ProviderCatalogEntry {
  id: string;
  displayName: string;
  cookieDomain: string | null;
}

export interface ProviderSummary {
  id: string;
  displayName: string;
  enabled: boolean;
  order: number;
}

export interface SettingsSnapshot {
  enabledProviders: string[];
  providerOrder?: string[];
  refreshIntervalSecs: number;
  /** Retry timeout failures three times before pausing background refresh. */
  /** Backend always supplies this; optional keeps older test/bootstrap fixtures compatible. */
  providerTimeoutRecoveryEnabled?: boolean;
  refreshAllProvidersOnMenuOpen: boolean;
  startAtLogin: boolean;
  startMinimized: boolean;
  showNotifications: boolean;
  soundEnabled: boolean;
  soundVolume: number;
  highUsageThreshold: number;
  criticalUsageThreshold: number;
  trayIconMode: TrayIconMode;
  switcherShowsIcons: boolean;
  menuBarShowsHighestUsage: boolean;
  showAsUsed: boolean;
  showAllTokenAccountsInMenu: boolean;
  enableAnimations: boolean;
  resetTimeRelative: boolean;
  menuBarDisplayMode: MenuBarDisplayMode;
  outputSpeedEnabled?: boolean;
  localUsagePeriod?: LocalUsagePeriod;
  hidePersonalInfo: boolean;
  updateChannel: UpdateChannel;
  autoDownloadUpdates: boolean;
  installUpdatesOnQuit: boolean;
  globalShortcut: string;
  /** Extra Codex home or sessions directories scanned for local cost estimates. */
  codexCustomSessionsDirs: string[];
  uiLanguage: Language;
  theme: ThemePreference;
  /** 100..=250 — clamped server-side. */

  /** 100..=200 — clamped server-side. */
  trayScalePercent: number;
  claudeAvoidKeychainPrompts: boolean;
  disableKeychainAccess: boolean;
  providerMetrics: Record<string, MetricPreference>;
  floatBarEnabled: boolean;
  /** 30..=100 — clamped server-side. */
  floatBarOpacity: number;
  /** 75..=200 — clamped server-side. */
  floatBarScale: number;
  floatBarOrientation: FloatBarOrientation;
  floatBarStyle: FloatBarStyle;
  floatBarClickThrough: boolean;
  /** Empty array = show all enabled providers. */
  floatBarProviderIds: string[];
  floatBarEntries?: TaskbarEntry[];
  /** When true, render with dark text/glass for light desktops. */
  floatBarDarkText: boolean;
  /** When true, render the next primary reset inline in each provider pill. */
  floatBarShowResetInline: boolean;
  /** Which windows' resets the bar prints, in order. Empty means none. */
  floatBarResetWindows: FloatBarResetWindow[];
  /** When true, show local cost summaries in the floating bar. */
  floatBarShowCost?: boolean;
  /** Windows only: embed a usage readout strip in the taskbar. */
  taskbarWidgetEnabled: boolean;
  /** Windows only: placement of the taskbar usage overlay. */
  taskbarWidgetPosition: TaskbarWidgetPosition;
  taskbarWidgetFontWeight: TaskbarWidgetFontWeight;
  /** Stroke weight for the self-drawn right-click menu. Own axis value,
   *  independent of the taskbar strip's. */
  menuFontWeight: number;
  menuFontFamily: string;
  menuFontSize: number;
  taskbarWidgetContent: TaskbarWidgetContent;
  /**
   * Ordered strip entries. Order is itself a setting: the strip has room for
   * only the first couple of lines, so position decides what survives.
   */
  taskbarWidgetEntries: TaskbarEntry[];
  taskbarWidgetFontFamily: string;
  taskbarWidgetFontSize: number;
  taskbarWidgetWidth: number;
  taskbarWidgetTextAlign: TaskbarWidgetTextAlign;
  taskbarWidgetIconSize?: number;
  taskbarWidgetIconStyle?: "pure" | "badge" | "solid";
  taskbarWidgetIconGapPx?: number;
  taskbarWidgetValueGapPx?: number;

  // ── Per-component quota presentation ───────────────────────────────
  //
  // Each surface owns its own used-vs-remaining and relative-vs-absolute
  // reset choice. `showAsUsed` / `resetTimeRelative` above are legacy
  // migration sources only — do not read them in new code.
  /** Floating bar: `true` shows used, `false` shows remaining. */
  floatBarShowAsUsed: boolean;
  /** Floating bar: `true` shows a countdown, `false` an absolute time. */
  floatBarResetTimeRelative: boolean;
  /** Tray flyout + PopOut panel: `true` shows used, `false` shows remaining. */
  dashboardShowAsUsed: boolean;
  /** Tray flyout + PopOut panel: countdown (`true`) or absolute time. */
  dashboardResetTimeRelative: boolean;
  /**
   * Legacy dashboard-only provider filter, retained for settings migration.
   * The dashboard now follows `enabledProviders` directly.
   */
  dashboardProviderIds: string[];
  /**
   * Legacy dashboard quota filter, retained for settings migration. Dashboard
   * cards now render the quota windows returned by each provider.
   */
  dashboardQuotaWindows: string[];
  /**
   * Windows taskbar strip and notification-area icon: `true` shows used,
   * `false` shows remaining. There is no `taskbarResetTimeRelative` companion —
   * the native strip renders no reset text, so such a setting would be inert.
   */
  taskbarShowAsUsed: boolean;
  /** Countdown (`true`) or the reset moment itself (`false`), for the strip
   *  family's surfaces — today that is the context menu's status row. */
  taskbarResetTimeRelative: boolean;
  /**
   * Ordered right-click actions for the mini status bar.
   * Known ids: open_panel | refresh | settings | quit.
   */
  /**
   * Hover/tooltip entries for the mini status bar (and tray tooltip when set),
   * independent of the painted strip entries. Empty reuses strip entries.
   */
  taskbarTooltipEntries: TaskbarEntry[];
}

/** Partial settings object — only include fields you want to change. */
export interface SettingsUpdate {
  enabledProviders?: string[];
  refreshIntervalSecs?: number;
  providerTimeoutRecoveryEnabled?: boolean;
  refreshAllProvidersOnMenuOpen?: boolean;
  startAtLogin?: boolean;
  startMinimized?: boolean;
  showNotifications?: boolean;
  soundEnabled?: boolean;
  soundVolume?: number;
  highUsageThreshold?: number;
  criticalUsageThreshold?: number;
  trayIconMode?: TrayIconMode;
  switcherShowsIcons?: boolean;
  menuBarShowsHighestUsage?: boolean;
  showAsUsed?: boolean;
  showAllTokenAccountsInMenu?: boolean;
  enableAnimations?: boolean;
  resetTimeRelative?: boolean;
  menuBarDisplayMode?: MenuBarDisplayMode;
  outputSpeedEnabled?: boolean;
  localUsagePeriod?: LocalUsagePeriod;
  hidePersonalInfo?: boolean;
  updateChannel?: UpdateChannel;
  autoDownloadUpdates?: boolean;
  installUpdatesOnQuit?: boolean;
  globalShortcut?: string;
  codexCustomSessionsDirs?: string[];
  uiLanguage?: Language;
  theme?: ThemePreference;
  trayScalePercent?: number;
  claudeAvoidKeychainPrompts?: boolean;
  disableKeychainAccess?: boolean;
  /** Map of provider CLI name → metric preference label. */
  providerMetrics?: Record<string, MetricPreference>;
  floatBarEnabled?: boolean;
  floatBarOpacity?: number;
  floatBarScale?: number;
  floatBarOrientation?: FloatBarOrientation;
  floatBarStyle?: FloatBarStyle;
  floatBarClickThrough?: boolean;
  floatBarProviderIds?: string[];
  floatBarEntries?: TaskbarEntry[];
  floatBarDarkText?: boolean;
  floatBarShowResetInline?: boolean;
  floatBarResetWindows?: FloatBarResetWindow[];
  floatBarShowCost?: boolean;
  taskbarWidgetEnabled?: boolean;
  taskbarWidgetPosition?: TaskbarWidgetPosition;
  taskbarWidgetFontWeight?: TaskbarWidgetFontWeight;
  menuFontWeight?: number;
  menuFontFamily?: string;
  menuFontSize?: number;
  taskbarWidgetContent?: TaskbarWidgetContent;
  taskbarWidgetEntries?: TaskbarEntry[];
  taskbarWidgetFontFamily?: string;
  taskbarWidgetFontSize?: number;
  taskbarWidgetWidth?: number;
  taskbarWidgetTextAlign?: TaskbarWidgetTextAlign;
  taskbarWidgetIconSize?: number;
  taskbarWidgetIconStyle?: "pure" | "badge" | "solid";
  taskbarWidgetIconGapPx?: number;
  taskbarWidgetValueGapPx?: number;
  floatBarShowAsUsed?: boolean;
  floatBarResetTimeRelative?: boolean;
  dashboardShowAsUsed?: boolean;
  dashboardResetTimeRelative?: boolean;
  /** Legacy dashboard-only filters; retained for migration compatibility. */
  dashboardProviderIds?: string[];
  dashboardQuotaWindows?: string[];
  taskbarShowAsUsed?: boolean;
  taskbarResetTimeRelative?: boolean;
  taskbarTooltipEntries?: TaskbarEntry[];
}

export interface BootstrapState {
  contractVersion: string;
  providers: ProviderCatalogEntry[];
  settings: SettingsSnapshot;
}

// ── Provider usage snapshot types ────────────────────────────────────

export interface RateWindowSnapshot {
  usedPercent: number;
  remainingPercent: number;
  /** Which cycle this is. See {@link QuotaCycleKind} — read it, never re-derive. */
  kind: QuotaCycleKind | null;
  windowMinutes: number | null;
  resetsAt: string | null;
  resetDescription: string | null;
  isExhausted: boolean;
  isInformational?: boolean;
  reservePercent: number | null;
  reserveDescription: string | null;
  reserveWillLastToReset?: boolean;
  reserveEtaSeconds?: number | null;
}

export interface CostSnapshotBridge {
  used: number;
  limit: number | null;
  remaining: number | null;
  currencyCode: string;
  period: string;
  resetsAt: string | null;
  formattedUsed: string;
  formattedLimit: string | null;
}

export interface PaceSnapshot {
  stage: "on_track" | "slightly_ahead" | "ahead" | "far_ahead" | "slightly_behind" | "behind" | "far_behind";
  deltaPercent: number;
  willLastToReset: boolean;
  etaSeconds: number | null;
  expectedUsedPercent: number;
  actualUsedPercent: number;
  /**
   * How many times the current burn rate could grow and still reach the reset.
   * `null` when the ratio is meaningless (nothing left, or nothing projected to
   * be spent) — never render a fabricated headroom figure.
   */
  speedMultiplierToReset: number | null;
}

export interface ProviderUsageSnapshot {
  providerId: string;
  displayName: string;
  primary: RateWindowSnapshot;
  primaryLabel?: string;
  secondary: RateWindowSnapshot | null;
  secondaryLabel?: string;
  modelSpecific: RateWindowSnapshot | null;
  tertiary: RateWindowSnapshot | null;
  extraRateWindows: Array<{
    id: string;
    title: string;
    window: RateWindowSnapshot;
    /**
     * False when the provider published the window without a known remaining
     * fraction (reset-only or disabled buckets). The window itself is
     * informational in that case; render it as unavailable, never as a real
     * 100%-remaining bar.
     */
    usageKnown?: boolean;
  }>;
  cost: CostSnapshotBridge | null;
  planName: string | null;
  accountEmail: string | null;
  sourceLabel: string;
  updatedAt: string;
  error: string | null;
  pace: PaceSnapshot | null;
  accountOrganization: string | null;
  trayStatusLabel: string | null;
  fetchDurationMs?: number | null;
  wayfinderUsage?: WayfinderUsageSnapshot | null;
}

export interface WayfinderRouteSummary {
  name: string;
  requests: number;
  tokens: number;
  realized: number;
  baseline: number;
  saved: number;
}

export interface WayfinderUsageSnapshot {
  gatewayStatus: string;
  offline: boolean;
  dryRun: boolean;
  missingKeys: string[];
  modelCount: number;
  models: string[];
  requests: number;
  estimatedRequests: number;
  tokens: number;
  realized: number;
  baseline: number;
  saved: number;
  savedPercent: number;
  periodDays: number;
  unit: string;
  priced: boolean;
  routes: WayfinderRouteSummary[];
}

export interface RefreshCompletePayload {
  providerCount: number;
  errorCount: number;
}

export interface SafeDiagnostics {
  appVersion: string;
  platform: string;
  enabledProviders: string[];
  providerCookieSources: Record<string, string>;
  hasManualCookies: string[];
  hasApiKeys: string[];
  hidePersonalInfo: boolean;
  refreshIntervalSecs: number;
}

export interface CredentialStorageStatus {
  manualCookies: string;
  apiKeys: string;
  tokenAccounts: string;
}

// ── Update state types ───────────────────────────────────────────────

export type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error";

export interface UpdateStatePayload {
  status: UpdateStatus;
  version: string | null;
  error: string | null;
  progress: number | null;
  releaseUrl: string | null;
  canDownload: boolean;
  canApply: boolean;
  /** Unix-ms timestamp of the last completed update check, or `null`
   *  if the app has not checked during this session. */
  lastCheckedAt: number | null;
}

// ── Credential store types ───────────────────────────────────────────

export interface ApiKeyInfoBridge {
  providerId: string;
  provider: string;
  maskedKey: string;
  savedAt: string;
  label: string | null;
}

export interface ApiKeyProviderInfoBridge {
  id: string;
  displayName: string;
  envVar: string | null;
  help: string | null;
  dashboardUrl: string | null;
}

export interface CookieInfoBridge {
  providerId: string;
  provider: string;
  savedAt: string;
}

/** Where the in-app login window was sent for a provider. */
export interface ProviderLoginTargetBridge {
  providerId: string;
  provider: string;
  url: string;
}

export interface ProviderOutputSpeed {
  providerId: "codex" | "claude";
  status: "generating" | "recent" | "unavailable";
  tokensPerSecond: number | null;
  outputTokens: number | null;
  updatedAtMs: number | null;
  approximate: boolean;
  recentSamples: OutputSpeedSample[];
}

export interface OutputSpeedSample {
  tokensPerSecond: number;
  outputTokens: number;
  durationMs: number;
  completedAtMs: number;
  model: string | null;
}

export interface OutputSpeedSnapshot {
  codex: ProviderOutputSpeed;
  claude: ProviderOutputSpeed;
  grok: ProviderOutputSpeed;
}

export interface CookieFileProviderBridge {
  providerId: string;
  provider: string;
  cookieCount: number;
}

export interface CookieFilePreviewBridge {
  format: string;
  providers: CookieFileProviderBridge[];
  unmatchedCookieCount: number;
}

export interface AppInfoBridge {
  name: string;
  version: string;
  buildNumber: string;
  updateChannel: string;
  tagline: string;
}

// ── Chart data types ─────────────────────────────────────────────────

export interface DailyCostPoint {
  date: string;
  value: number;
}

export interface ServiceUsagePoint {
  service: string;
  creditsUsed: number;
}

export interface DailyUsageBreakdown {
  day: string;
  services: ServiceUsagePoint[];
  totalCreditsUsed: number;
}

export interface ProviderLocalUsageSummary {
  todayCost: number | null;
  todayTokens: number | null;
  sevenDayCost: number | null;
  sevenDayTokens: number | null;
  thirtyDayCost: number | null;
  thirtyDayTokens: number | null;
  todayTopModel: string | null;
  sevenDayTopModel: string | null;
  thirtyDayTopModel: string | null;
  estimateNote: string;
}

export interface ProviderChartData {
  providerId: string;
  costHistory: DailyCostPoint[];
  creditsHistory: DailyCostPoint[];
  usageBreakdown: DailyUsageBreakdown[];
  localUsage: ProviderLocalUsageSummary | null;
}

// ── Token account types ──────────────────────────────────────────────

export interface TokenAccountSupportBridge {
  providerId: string;
  displayName: string;
  title: string;
  subtitle: string;
  placeholder: string;
}

export interface TokenAccountBridge {
  id: string;
  label: string;
  addedAt: string;
  lastUsed: string | null;
  isActive: boolean;
}

export interface ProviderTokenAccountsBridge {
  providerId: string;
  support: TokenAccountSupportBridge;
  accounts: TokenAccountBridge[];
  activeIndex: number;
}

// ── Phase 4 — provider ordering / cookie source / region ─────────────

export interface ProviderSummary {
  id: string;
  displayName: string;
  enabled: boolean;
  order: number;
}

// ── Phase 4 — credential detection ───────────────────────────────────

export interface GeminiCliStatus {
  signedIn: boolean;
  credentialsPath: string | null;
}

export interface VertexAiStatus {
  hasCredentials: boolean;
  credentialsPath: string | null;
}

export interface JetbrainsIde {
  id: string;
  displayName: string;
  path: string;
  detected: boolean;
}

export interface KiroStatus {
  available: boolean;
  hint: string | null;
}

// ── Phase 4 — session / environment ──────────────────────────────────

export interface WorkAreaRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

// ── Phase 4 — event payloads ─────────────────────────────────────────

/** Payload emitted for the `global-shortcut-triggered` event: the
 *  accelerator string that fired, e.g. `"Ctrl+Shift+U"`. */
export type GlobalShortcutTriggeredPayload = string;

// ── Phase 5 — i18n ────────────────────────────────────────────────────

/** Snapshot returned by `get_locale_strings`. */
export interface LocaleStrings {
  language: Language;
  entries: Record<string, string>;
}

/** Payload emitted for `locale-changed`: the persisted language label. */
export type LocaleChangedPayload = Language;

// ── Phase 6b — provider detail pane ──────────────────────────────────

/** Aggregated per-provider payload powering the Settings detail pane. */
export interface ProviderDetail {
  id: string;
  displayName: string;
  enabled: boolean;

  // Identity
  email: string | null;
  plan: string | null;
  authType: string | null;
  sourceLabel: string | null;
  organization: string | null;
  lastUpdated: string | null;

  // Usage windows — mirror RateWindowSnapshot.
  session: RateWindowSnapshot | null;
  weekly: RateWindowSnapshot | null;
  modelSpecific: RateWindowSnapshot | null;
  tertiary: RateWindowSnapshot | null;
  extraRateWindows: Array<{
    id: string;
    title: string;
    window: RateWindowSnapshot;
    /** False when the provider published the window without a known remaining
     *  fraction; render as unavailable, never as a 100%-remaining bar. */
    usageKnown?: boolean;
  }>;

  cost: CostSnapshotBridge | null;
  pace: PaceSnapshot | null;

  lastError: string | null;

  dashboardUrl: string | null;
  statusPageUrl: string | null;
  buyCreditsUrl: string | null;

  hasSnapshot: boolean;

  /** Phase 6c — currently-persisted cookie source value ("auto" | "manual" | "off" | …).
   *  `null` for providers that do not expose a cookie-source picker. */
  cookieSource: string | null;
  /** Phase 6c — currently-persisted region value. `null` for non-regional providers. */
  region: string | null;
}

// ── Phase 6c — cookie-source & region pickers ────────────────────────

export interface CookieSourceOption {
  value: string;
  label: string;
  description?: string;
}

export interface RegionOption {
  value: string;
  label: string;
}

/**
 * Per-provider authentication capability, mirroring the shared `Provider`
 * trait (`supports_oauth` / `supports_cli` / `supports_web`) plus the
 * API-key catalog and `ProviderId::cookie_domain()`. Drives which entries the
 * Settings → Providers "认证来源" block offers — read-only, presentation only.
 */
export interface ProviderAuthCapabilitiesBridge {
  supportsOAuth: boolean;
  supportsCli: boolean;
  supportsWeb: boolean;
  supportsApiKey: boolean;
  hasCookieDomain: boolean;
}

// ── Phase 6d — credential detection ──────────────────────────────────

export interface GeminiCliStatus {
  signedIn: boolean;
  credentialsPath: string | null;
}

export interface VertexAiStatus {
  hasCredentials: boolean;
  credentialsPath: string | null;
}

export interface JetbrainsIde {
  id: string;
  displayName: string;
  path: string;
  detected: boolean;
}

export interface KiroStatus {
  available: boolean;
  hint: string | null;
}
