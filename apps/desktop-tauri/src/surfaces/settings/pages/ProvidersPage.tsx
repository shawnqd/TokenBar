import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import Sortable from "sortablejs";
import { useDragAutoScroll } from "../../../hooks/useDragAutoScroll";
import { ProviderIcon } from "../../../components/providers/ProviderIcon";
import { getProviderIcon } from "../../../components/providers/providerIcons";
import type { ProviderSnapshot } from "../../../core/snapshot";
import { projectSurface } from "../../../core/projection";
import type { UsageStore } from "../../../core/usageStore";
import { useActionDispatcher, useDispatchAction } from "../../../core/useCoreBridge";
import { requireActionResult, type ActionDispatcher } from "../../../core/actionDispatcher";
import {
  quotaDisplayContext,
  quotaPercentContext,
} from "../../../lib/quotaDisplay";
import {
  getProviderRegionOptions,
  getProviderRegion,
  getApiKeys,
  getGeminiCliSignedIn,
  getProviderDetail,
  getManualCookies,
  getProviderAuthCapabilities,
  getProviderCookieSource,
  getProviderCookieSourceOptions,
  getTokenAccountProviders,
  previewCookieFile,
} from "../../../lib/tauri";
import type {
  MetricPreference,
  ProviderDetail,
  ApiKeyInfoBridge,
  CookieInfoBridge,
  CookieSourceOption,
  CookieFilePreviewBridge,
  RegionOption,
  ProviderAuthCapabilitiesBridge,
} from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import ProviderActionBar from "./ProviderActionBar";
import ProviderUsageCard from "./ProviderUsageCard";
import { TokenAccountsPanel } from "../tokens/TokenAccountsPanel";
import {
  COOKIE_IMPORT_ID,
  METHOD_LABEL,
  buildProviderCatalog,
  type AuthMethod,
  type FixtureProvider,
} from "./htmlFixture";
import { ConfirmDialog, V5Select, V5Toggle } from "./v5Controls";

function listUpdatedLabel(updatedMs: number, nowMs: number): string {
  const diffSecs = Math.max(0, Math.floor((nowMs - updatedMs) / 1000));
  if (diffSecs < 60) return "刚刚更新";
  const diffMins = Math.floor(diffSecs / 60);
  if (diffMins < 60) return `${diffMins} 分钟前`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours} 小时前`;
  return `${Math.floor(diffHours / 24)} 天前`;
}

/** Displayed in the key field while a saved key exists; click selects all so
 *  a fresh paste replaces it. Purely a display mask — saving is guarded so the
 *  untouched mask can never be written back as a key. */
const API_KEY_MASK = "＊＊＊＊＊＊";

/** Labels for the runtime usage-source catalog (upstream ProviderSource*Short
 *  vocabulary: 自动/网页/CLI/OAuth). Keys are the normalized backend values. */
const USAGE_SOURCE_LABEL: Record<string, string> = {
  auto: "自动选择",
  web: "网页数据",
  cli: "本机 CLI",
  oauth: "OAuth 接口",
};

function AuthBody({
  provider,
  method,
  hasApiKey,
  hasCookie,
  isLoggedIn,
  loginFlow,
  hasLoginLadder,
  cookieSource,
  cookieSourceOptions,
  onCookieSourceChanged,
  onNote,
  onConfirm,
  onCredentialChange,
}: {
  provider: FixtureProvider;
  method: AuthMethod;
  hasApiKey: boolean;
  hasCookie: boolean;
  isLoggedIn: boolean;
  loginFlow: string | null;
  hasLoginLadder: boolean;
  cookieSource: string | null;
  cookieSourceOptions: CookieSourceOption[];
  onCookieSourceChanged: (value: string) => void;
  onNote: (text: string) => void;
  onConfirm: (text: string, action: () => void) => void;
  onCredentialChange: () => void;
}) {
  // Spec §4d: the login button must name the real action (CLI 登录 / 设备
  // 登录), never a generic "打开登录" — `loginFlow` is the only source of a
  // user-facing login action.
  const loginActionLabel = loginFlow?.endsWith("_device")
    ? "设备登录"
    : loginFlow?.endsWith("_oauth")
      ? "OAuth 登录"
      : loginFlow?.endsWith("_cli")
        ? "CLI 登录"
        : "开始登录";
  const loginLead = loginFlow?.endsWith("_device")
    ? `用设备码完成 ${provider.name} 登录；不要在这里粘贴密钥或 Cookie。`
    : loginFlow?.endsWith("_oauth")
      ? `完成 ${provider.name} 的 OAuth 授权；不要在这里粘贴密钥或 Cookie。`
      : loginFlow?.endsWith("_cli")
        ? `在本机运行 ${provider.name} 的命令行工具完成登录；不要在这里粘贴密钥或 Cookie。`
        : `按 ${provider.name} 提供的登录流程完成登录；不要在这里粘贴密钥或 Cookie。`;
  const dispatch = useDispatchAction();
  const [cookieText, setCookieText] = useState("");
  const [keyLabel, setKeyLabel] = useState("");
  const [keyValue, setKeyValue] = useState("");
  const [keyDirty, setKeyDirty] = useState(false);

  if (method === "cookie") {
    // Upstream parity (2026-09-05): `manual` is the default source — the
    // provider reads a session the user supplied on purpose; automatic
    // browser-cookie reading is an explicit opt-in. One card owns the whole
    // web-session story (2026-09-06 user decision): the read-source picker
    // (upstream CookieSourceSection) leads, the pasted-session editor
    // (upstream CookieSection) follows — both always visible. The generic
    // WebView capture entry stays absent.
    const selectedSource =
      cookieSource?.trim() || cookieSourceOptions[0]?.value || "auto";
    const currentOption = cookieSourceOptions.find(
      (option) => option.value === selectedSource,
    );
    const sourceExplanation = currentOption?.description?.trim();
    const sourceLead = currentOption
      ? `${currentOption.label}${sourceExplanation ? ` — ${sourceExplanation}` : ""}`
      : "手动 — 使用你保存或导入的 Cookie；要自动读取浏览器 Cookie 请选「自动读取浏览器」。";
    // 粘贴编辑器属于「使用已保存 Cookie」分支：自动读取浏览器 / 停用来源时
    // 隐藏（已保存会话的状态与删除保持可见，避免无法撤销）。已保存与未保存
    // 在状态徽标与输入框描边上做区分。
    const editorVisible = selectedSource === "manual" || cookieSourceOptions.length === 0;
    const lead = (() => {
      if (selectedSource === "auto") {
        // 已保存会话在自动模式下仍按阶梯优先，但其管理 UI 属于手动分支；
        // 这里只用一句话交代优先级，并指出切回哪里管理。
        return hasCookie
          ? `自动读取浏览器中的 ${provider.name} Cookie；已保存的 Cookie 仍会优先使用，切回「使用已保存 Cookie」可管理。`
          : `自动读取浏览器中的 ${provider.name} Cookie。`;
      }
      if (selectedSource === "off") {
        return hasLoginLadder
          ? `已停用 ${provider.name} 的 Cookie 来源；刷新将走该服务商自己的登录方式（CLI / OAuth）。`
          : `已停用 ${provider.name} 的 Cookie 来源。`;
      }
      return `默认使用你在这里保存或导入的 ${provider.name} Cookie 来刷新用量。${
        hasLoginLadder ? "没有保存时会走该服务商自己的登录方式（CLI / OAuth）。" : ""
      }`;
    })();
    return (
      <div className="s5-login-hero">
        <p>{lead}</p>
        {editorVisible ? (
        <div className="s5-pd-field">
          <div className="s5-pd-field-head">
            <div className="s5-field-label">已保存 Cookie</div>
            {hasCookie ? (
              <div className="s5-login-ok">已保存</div>
            ) : (
              <div className="s5-login-ok is-missing">未保存</div>
            )}
          </div>
          <textarea
            className={`s5-area${hasCookie ? " is-saved" : " is-empty"}`}
            placeholder={hasCookie ? "已有一份。粘贴新的会覆盖旧的，保存后不会回显" : "粘贴 Cookie 请求头，保存后不会回显"}
            value={cookieText}
            onChange={(event) => setCookieText(event.target.value)}
          />
        </div>
        ) : null}
        {editorVisible ? (
        <div className="s5-actions">
          <button
            type="button"
            className="s5-ghost"
            onClick={() =>
              void requireActionResult(
                dispatch({
                  type: "setManualCookie",
                  target: { kind: "provider", providerId: provider.id },
                  cookieHeader: cookieText.trim(),
                }),
              )
                .then(() => {
                  setCookieText("");
                  onNote("已保存 Cookie");
                  onCredentialChange();
                })
                .catch((error) => onNote(String(error)))
            }
          >
            保存
          </button>
          {hasCookie ? (
            <button
              type="button"
              className="s5-ghost danger"
              onClick={() =>
                onConfirm(`删除后需重新登录 ${provider.name}。`, () =>
                  void requireActionResult(
                    dispatch({
                      type: "removeManualCookie",
                      target: { kind: "provider", providerId: provider.id },
                    }),
                  )
                    .then(() => {
                      onNote("已删除 Cookie");
                      onCredentialChange();
                    })
                    .catch((error) => onNote(String(error))),
                )
              }
            >
              删除已保存 Cookie
            </button>
          ) : null}
        </div>
        ) : null}
        {editorVisible ? (
          <p className="s5-pd-lead">
            没有现成的 Cookie？用左边「批量导入浏览器 Cookie」一次导入多家；这些 Cookie 只保存在本机。
          </p>
        ) : null}
      </div>
    );
  }

  if (method === "signIn") {
    const gemini = provider.id === "gemini";
    const isCodex = provider.id === "codex";
    // Spec v2.2.2: per-provider lead copy; Codex must say CLI/OAuth, never a
    // system-browser flow. The runtime has no multi-account switching, so a
    // signed-in provider offers 重新登录, never 切换账号.
    const lead = gemini
      ? "如果本机已经用 Gemini / Antigravity 登录过，点检测即可，不用填密钥。"
      : isCodex
        ? "通过 Codex CLI 或 OAuth 完成登录。本页不需要粘贴密钥或 Cookie。"
        : loginLead;
    const readyBadge = loginFlow?.endsWith("_oauth")
      ? "OAuth 授权已就绪"
      : "本机 CLI 登录已就绪";
    return (
      <div className="s5-login-hero">
        <p>{lead}</p>
        {isLoggedIn ? <div className="s5-login-ok">{readyBadge}</div> : null}
        <div className="s5-actions">
          <button
            type="button"
            className="s5-primary"
            onClick={() =>
              void (gemini
                ? getGeminiCliSignedIn()
                : dispatch({
                    type: "triggerLogin",
                    target: { kind: "provider", providerId: provider.id },
                  }))
                .then(() => onNote(gemini ? "检测完成" : "已发起登录"))
                .catch((error) => onNote(String(error)))
            }
          >
            {gemini
              ? "检测本机登录"
              : isLoggedIn
                ? "重新登录"
                : isCodex
                  ? "运行 Codex 登录"
                  : loginActionLabel}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="s5-login-hero">
      <p>使用 API 密钥获取用量。保存后会自动刷新。</p>
      {hasApiKey ? <div className="s5-login-ok">API 密钥已保存</div> : null}
      {!hasApiKey ? <p className="s5-pd-lead">还没有保存 API 密钥。</p> : null}
      <div className="s5-pd-form">
        <div className="s5-pd-row">
          <div className="s5-field-label">标签</div>
          <input
            className="s5-textin"
            value={keyLabel}
            onChange={(event) => setKeyLabel(event.target.value)}
            placeholder="例如 生产"
          />
        </div>
        <div className="s5-pd-row">
          <div className="s5-field-label">API 密钥</div>
          <input
            className="s5-textin"
            type="password"
            value={keyDirty ? keyValue : hasApiKey ? API_KEY_MASK : keyValue}
            onFocus={(event) => event.currentTarget.select()}
            onChange={(event) => {
              setKeyValue(event.target.value);
              setKeyDirty(true);
            }}
            placeholder="粘贴后不会回显"
          />
        </div>
        <div className="s5-actions">
          <button
            type="button"
            className="s5-primary"
            onClick={() => {
              // The untouched mask is a display placeholder, not a key — only
              // a genuinely typed/pasted value may be written back.
              const nextKey = keyDirty ? keyValue.trim() : "";
              if (!nextKey) {
                onNote(hasApiKey ? "API 密钥未改动" : "请先粘贴 API 密钥");
                return;
              }
              void requireActionResult(
                dispatch({
                  type: "setApiKey",
                  target: { kind: "provider", providerId: provider.id },
                  apiKey: nextKey,
                  label: keyLabel.trim() || undefined,
                }),
              )
                .then(() => {
                  setKeyValue("");
                  setKeyLabel("");
                  setKeyDirty(false);
                  onNote("已保存 API 密钥");
                  onCredentialChange();
                })
                .catch((error) => onNote(String(error)));
            }}
          >
            保存
          </button>
          {hasApiKey ? (
            <button
              type="button"
              className="s5-ghost danger"
              onClick={() =>
                onConfirm(`删除 ${provider.name} 的 API 密钥，不影响其他服务商。`, () =>
                  void requireActionResult(
                    dispatch({
                      type: "removeApiKey",
                      target: { kind: "provider", providerId: provider.id },
                    }),
                  )
                    .then(() => {
                      onNote("已删除 API 密钥");
                      onCredentialChange();
                    })
                    .catch((error) => onNote(String(error))),
                )
              }
            >
              删除 API 密钥
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function TokenCard({
  providerId,
  hasToken,
  onConfirm,
}: {
  providerId: string;
  hasToken: boolean;
  onConfirm: (text: string, action: () => void) => void;
}) {
  if (!hasToken) return null;

  return (
    <div className="s5-pd-card">
      <div className="s5-pd-card-h">
        <h4>账号</h4>
      </div>
      <TokenAccountsPanel providerId={providerId} compact onConfirm={onConfirm} />
    </div>
  );
}

const EMPTY_STORE_STATE: { version: number; records: Record<string, unknown> } = {
  version: 0,
  records: {},
};
function useCoreSnapshotListForPage(store?: UsageStore | null): ProviderSnapshot[] {
  const subscribe = useCallback(
    (cb: () => void) => (store ? store.subscribe(cb) : () => {}),
    [store],
  );
  const getSnapshot = useCallback(
    () => (store ? store.getSnapshot() : EMPTY_STORE_STATE),
    [store],
  );
  const getServerSnapshot = useCallback(() => getSnapshot(), [getSnapshot]);
  const state = useSyncExternalStore(
    subscribe as unknown as Parameters<typeof useSyncExternalStore>[0],
    getSnapshot as never,
    getServerSnapshot as never,
  ) as unknown as { records: Record<string, { snapshot: ProviderSnapshot | null }> };
  return useMemo(() => Object.values(state.records).map((r) => r.snapshot).filter(Boolean) as ProviderSnapshot[], [state]);
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${Math.max(1, Math.round(kb))} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

function ImportCheckMark() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
      <path
        d="M3.2 8.2 6.3 11.2 12.8 4.6"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export default function ProvidersPage({
  settings,
  set,
  saving,
  catalog: liveCatalog,
  coreStore: injectedCoreStore,
  dispatcher: injectedDispatcher,
}: SettingsPageProps & { coreStore?: UsageStore | null; dispatcher?: ActionDispatcher | null }) {
  const enabled = settings.enabledProviders ?? [];
  const motion = settings.enableAnimations !== false;
  const coreSnapshots = useCoreSnapshotListForPage(injectedCoreStore);
  const snapshotById = useMemo(
    () => new Map(coreSnapshots.map((row) => [row.providerId, row])),
    [coreSnapshots],
  );
  const actionDispatcher = useActionDispatcher(injectedDispatcher ?? undefined);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(
    () => enabled[0] ?? liveCatalog?.[0]?.id ?? "",
  );
  const setSelectedWrapped = useCallback(
    (id: string) => {
      const d = actionDispatcher as unknown as ActionDispatcher;
      if (d && typeof d.dispatch === "function") {
        void d.dispatch({ type: "openProviderDetail", target: { kind: "provider", providerId: id } } as never).catch(() => {});
      }
      setSelected(id);
    },
    [actionDispatcher],
  );

  useEffect(() => {
    const timer = window.setInterval(() => {
      setNowMs(Date.now());
    }, 60_000);
    return () => window.clearInterval(timer);
  }, []);
  const [authById, setAuthById] = useState<Record<string, AuthMethod>>({});
  // Real per-provider auth capabilities from the backend (G7). Null until the
  // first batch resolves; the login card shows a loading state instead of
  // fabricating methods from a static catalog.
  const [authCaps, setAuthCaps] = useState<
    Record<string, ProviderAuthCapabilitiesBridge | null> | null
  >(null);
  // Token-account support comes from the runtime registry
  // (`get_token_account_providers`), never from a hardcoded provider set.
  const [tokenAccountIds, setTokenAccountIds] = useState<string[] | null>(null);
  const [cookieSource, setCookieSource] = useState<string | null>(null);
  const [cookieSourceOptions, setCookieSourceOptions] = useState<CookieSourceOption[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{
    text: string;
    action: () => void;
  } | null>(null);
  const [cookieFile, setCookieFile] = useState(false);
  const [cookieFileContents, setCookieFileContents] = useState("");
  const [cookieFileName, setCookieFileName] = useState("");
  const [cookieFileSize, setCookieFileSize] = useState(0);
  const [cookiePhase, setCookiePhase] = useState<"empty" | "preview" | "confirm" | "done">("empty");
  const [cookieWritten, setCookieWritten] = useState<Array<{ id: string; name: string }>>([]);
  const [stepsOpen, setStepsOpen] = useState(false);
  const [unmatchedOpen, setUnmatchedOpen] = useState(false);
  const cookieInputRef = useRef<HTMLInputElement>(null);
  // Batch-import preview (backend `preview_cookie_file`): which providers the
  // chosen file actually matches, so the write step is a confirmation of real
  // matches instead of a blind list of every cookie-capable provider.
  const [cookiePreview, setCookiePreview] = useState<CookieFilePreviewBridge | null>(null);
  const [cookiePreviewBusy, setCookiePreviewBusy] = useState(false);
  const [cookiePreviewError, setCookiePreviewError] = useState<string | null>(null);
  const [cookieDragOver, setCookieDragOver] = useState(false);
  // Absent key means "checked by default" so late-arriving catalog entries
  // don't need to be merged into the toggle record.
  const [cookieTargets, setCookieTargets] = useState<Record<string, boolean>>({});
  const [apiKeys, setApiKeys] = useState<ApiKeyInfoBridge[]>([]);
    const [manualCookies, setManualCookies] = useState<CookieInfoBridge[]>([]);
  const [detail, setDetail] = useState<ProviderDetail | null>(null);
  const [regionOptions, setRegionOptions] = useState<RegionOption[]>([]);
  const [regionValue, setRegionValue] = useState("");
  const [regionError, setRegionError] = useState<string | null>(null);
  const [regionBusy, setRegionBusy] = useState(false);

    const reloadCredentials = useCallback(async () => {
      try {
        const [keys, cookies] = await Promise.all([
          getApiKeys(),
          getManualCookies(),
        ]);
        setApiKeys(keys);
        setManualCookies(cookies);
      } catch {
        // Keep last known state; the UI will show unconfigured rather than crash.
      }
    }, []);

    useEffect(() => {
      void reloadCredentials();
    }, [reloadCredentials]);

  useEffect(() => {
    let cancelled = false;
    getTokenAccountProviders()
      .then((rows) => {
        if (!cancelled) setTokenAccountIds(rows.map((row) => row.providerId));
      })
      .catch(() => {
        // Registry read failed: hide token cards instead of guessing which
        // providers support accounts.
        if (!cancelled) setTokenAccountIds([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // The capability command is a pure local registry read, so the whole
  // catalog can be resolved up front and the auth entries never depend on
  // which provider the user happened to open first.
  const catalogKey = useMemo(
    () => (liveCatalog ?? []).map((row) => row.id).join(","),
    [liveCatalog],
  );
  useEffect(() => {
    const ids = (liveCatalog ?? []).map((row) => row.id);
    if (ids.length === 0) return;
    let cancelled = false;
    void Promise.all(
      ids.map((id) =>
        getProviderAuthCapabilities(id)
          .then((caps) => [id, caps] as const)
          .catch(() => [id, null] as const),
      ),
    ).then((entries) => {
      if (cancelled) return;
      setAuthCaps(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
  }, [catalogKey, liveCatalog]);


  const handleActionError = useCallback((message: string | null) => {
    setNote(message);
  }, []);
  const handleDetail = useCallback((next: ProviderDetail | null) => {
    setDetail(next);
  }, []);

  const catalog = useMemo(
    () =>
      buildProviderCatalog(
        liveCatalog,
        enabled,
        settings.providerOrder ?? [],
        authCaps ?? undefined,
      ),
    [enabled, liveCatalog, settings.providerOrder, authCaps],
  );

  // Cookie 文件上限与后端 `parse_cookie_file` 前的预览一致（2 MB）。
  const MAX_COOKIE_FILE_BYTES = 2 * 1024 * 1024;

  const resetCookieFile = useCallback(() => {
    setCookieFile(false);
    setCookieFileContents("");
    setCookieFileName("");
    setCookieFileSize(0);
    setCookiePreview(null);
    setCookiePreviewError(null);
    setCookiePreviewBusy(false);
    setCookieTargets({});
    setCookiePhase("empty");
    setCookieWritten([]);
    setUnmatchedOpen(false);
  }, []);

  // Read a chosen file, enforce the size cap locally, then ask the backend
  // which providers the file actually matches. Every failure path lands in
  // `cookiePreviewError` so the dropzone area always explains itself.
  const loadCookieFile = useCallback(async (file: File) => {
    setCookiePreviewError(null);
    setCookiePreview(null);
    setCookieTargets({});
    if (file.size > MAX_COOKIE_FILE_BYTES) {
      setCookieFile(false);
      setCookieFileContents("");
      setCookieFileName("");
      setCookiePreviewError(
        `文件超过 2 MB 限制（${(file.size / 1024 / 1024).toFixed(1)} MB）。请在扩展导出时只勾选需要的站点。`,
      );
      return;
    }
    const text = await file.text();
    setCookieFileContents(text);
    setCookieFileName(file.name);
    setCookieFileSize(file.size);
    setCookieFile(true);
    setCookiePhase("preview");
    setCookiePreviewBusy(true);
    try {
      const preview = await previewCookieFile(text);
      setCookiePreview(preview);
      setUnmatchedOpen(false);
    } catch (error) {
      setCookieFile(false);
      setCookiePreview(null);
      setCookiePhase("empty");
      setCookiePreviewError(
        typeof error === "string" ? error : `无法解析这份文件：${String(error)}`,
      );
    } finally {
      setCookiePreviewBusy(false);
    }
  }, []);

  const savedCookieIds = useMemo(
    () => new Set(manualCookies.map((cookie) => cookie.providerId)),
    [manualCookies],
  );

  const commitCookieImport = useCallback(
    (ids: string[]) => {
      const written = ids.map((id) => {
        const row = catalog.find((item) => item.id === id);
        const entry = cookiePreview?.providers.find((item) => item.providerId === id);
        return { id, name: row?.name ?? entry?.provider ?? id };
      });
      void requireActionResult(
        (actionDispatcher as ActionDispatcher).dispatch({
          type: "importCookieFile",
          target: { kind: "settings" },
          contents: cookieFileContents,
          providerIds: ids,
        }),
      )
        .then(() => {
          setCookieWritten(written);
          setCookiePhase("done");
          void reloadCredentials();
        })
        .catch((error) => setNote(String(error)));
    },
    [actionDispatcher, catalog, cookieFileContents, cookiePreview, reloadCredentials],
  );

  const visible = catalog.filter((p) => {
    if (!query.trim()) return true;
    const q = query.trim().toLowerCase();
    return p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q);
  });

    const rowsRef = useRef<HTMLDivElement>(null);
    const [dragActive, setDragActive] = useState(false);
    useDragAutoScroll(rowsRef, dragActive);

    useEffect(() => {
      const el = rowsRef.current;
      if (!el || saving) return;
      // 用成熟排序库 sortablejs，不自研动画：跟手 + 让位 + 归位全靠库。
      const sortable = Sortable.create(el, {
        animation: 150,
        draggable: ".s5-prow",
        filter: ".s5-pinned",
        preventOnFilter: false,
        ghostClass: "s5-prow--ghost",
        chosenClass: "s5-prow--dragging",
        // Sortable 内置自动滚动在指针离开列表（如移到上方搜索框）的瞬间会
        // 清空全部滚动定时器，向上拖动因此滚不动；滚动交给
        // useDragAutoScroll 在 window 级跟踪指针并连续处理。
        scroll: false,
        onStart: () => setDragActive(true),
        onEnd: () => setDragActive(false),
        onUpdate: () => {
          const ordered = Array.from(
            el.querySelectorAll<HTMLElement>(":scope > .s5-prow[data-prow-id]"),
          ).map((node) => node.getAttribute("data-prow-id") ?? "");
          const nextVisible = ordered.filter(Boolean);
          if (nextVisible.length === 0) return;
          const visibleIds = new Set(nextVisible);
          const nextIds = catalog
            .map((p) => p.id)
            .map((id) => (visibleIds.has(id) ? (nextVisible.shift() ?? id) : id));
          void (actionDispatcher as ActionDispatcher)
            .dispatch({
              type: "reorderProviders",
              target: { kind: "summary" },
              providerIds: nextIds,
            })
            .catch(() => setNote("排序保存失败"));
        },
      });
      return () => sortable.destroy();
    }, [saving, catalog]);

  const selectedProvider = catalog.find((p) => p.id === selected);
  useEffect(() => {
    if (!selectedProvider) {
      setRegionOptions([]);
      setRegionValue("");
      setRegionError(null);
      return;
    }
    let cancelled = false;
    setRegionError(null);
    void Promise.all([
      getProviderRegionOptions(selectedProvider.id),
      getProviderRegion(selectedProvider.id),
      getProviderCookieSource(selectedProvider.id).catch(() => null),
      getProviderCookieSourceOptions(selectedProvider.id).catch(() => []),
    ])
      .then(([options, persisted, source, sourceOptions]) => {
        if (cancelled) return;
        setRegionOptions(options);
        // The region row renders only when the backend reports region
        // options; there is no static per-provider region placeholder.
        setRegionValue(persisted?.trim() || options[0]?.value || "");
        setCookieSource(source ?? null);
        setCookieSourceOptions(sourceOptions);
      })
      .catch((error) => {
        if (cancelled) return;
        setRegionOptions([]);
        setRegionError(String(error));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedProvider?.id]);
  // Auth capabilities resolved: empty methods means the backend reported no
  // supported entry — never fall back to a fabricated method. While the
  // first batch is still loading, surface the login card's loading state.
  const capsLoading = authCaps === null;
  const method: AuthMethod | null =
    selectedProvider == null || capsLoading
      ? null
      : (authById[selectedProvider.id] ?? selectedProvider.primary);

    const selectedHasApiKey = selectedProvider
      ? apiKeys.some((key) => key.providerId === selectedProvider.id)
      : false;
    const selectedHasCookie = selectedProvider
      ? manualCookies.some((cookie) => cookie.providerId === selectedProvider.id)
      : false;
    const selectedSnapshot = selectedProvider
      ? (snapshotById.get(selectedProvider.id) as { error?: string | null } | undefined)
      : undefined;
    const selectedIsLoggedIn = Boolean(
      selectedSnapshot && !(selectedSnapshot as { error?: string | null }).error,
    );

  const toggleEnabled = (id: string, next: boolean) => {
    const current = new Set(enabled);
    if (next) current.add(id);
    else current.delete(id);
    set({ enabledProviders: [...current] });
  };

  const rowMeta = (id: string, isOn: boolean) => {
    const snap = snapshotById.get(id) ?? null;
    if (!isOn) return { sub: "未配置", metric: "", problem: false };
    if (!snap || snap.error || snap.displayState === "error" || snap.displayState === "authRequired") {
      return { sub: "未配置", metric: "", problem: Boolean(snap?.error) };
    }
    const updatedMs = snap.updatedAt ? Date.parse(snap.updatedAt) : NaN;
    const time = Number.isFinite(updatedMs) ? listUpdatedLabel(updatedMs, nowMs) : "刚刚更新";
    // Spec §6.2 list vocabulary: 「已登录 · 时间」 for healthy rows, 「未配置」 otherwise.
    const sub = `已登录 · ${time}`;
    let metric = "";
    const showAsUsed = quotaPercentContext(settings, "dashboard").showAsUsed;
    const proj = projectSurface(snap, { showAsUsed });
    if (proj.primary && proj.primary.fillPercent != null) {
      metric = `${Number(proj.primary.fillPercent.toFixed(1))}%`;
    } else if (proj.balance) {
      metric = proj.balance.amountText;
    }
    const stale = Number.isFinite(updatedMs) && nowMs - updatedMs > 10 * 60_000;
    return { sub, metric, problem: stale || snap.displayState === "stale" };
  };

  const who = selectedProvider
    ? [
        detail?.plan,
        settings.hidePersonalInfo ? null : (detail?.email ?? null),
      ]
        .filter(Boolean)
        .join(" · ") ||
      (selectedProvider.enabled
        ? method
          ? METHOD_LABEL[method]
          : capsLoading
            ? "正在读取登录方式"
            : "登录方式不可用"
        : "未登录")
    : "";

  return (
    <div className="s5-prov">
      <div className="s5-plist">
        <div className="s5-plist-top">
          <h2>服务商</h2>
          <div className="s5-search-wrap">
            <input
              className="s5-search"
              placeholder="搜索名称"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            {query ? (
              <button
                type="button"
                className="s5-search-clear"
                onClick={() => setQuery("")}
              >
                ×
              </button>
            ) : null}
          </div>
        </div>
        <div ref={rowsRef} className="s5-prov-rows">
          <button
            type="button"
            className={`s5-pinned${selected === COOKIE_IMPORT_ID ? " on" : ""}`}
            onClick={() => setSelectedWrapped(COOKIE_IMPORT_ID)}
          >
            <span className="s5-pinned-icon" aria-hidden="true">
              <svg
                viewBox="0 0 16 16"
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.4"
              >
                <path
                  d="M8 10V2.5M8 2.5L5.5 5M8 2.5L10.5 5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path d="M2.5 10.5v2a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-2" strokeLinecap="round" />
              </svg>
            </span>
            <span className="s5-pinned-text">
              <span className="s5-pinned-title">批量导入浏览器 Cookie</span>
            </span>
          </button>
          {catalog.length === 0 ? (
            // Runtime catalog missing (bootstrap not resolved or backend
            // failure): show an honest empty state, never a static list.
            <p className="s5-hint">服务商目录尚未加载。刷新页面或重启应用后重试。</p>
          ) : visible.length === 0 ? (
            <p className="s5-hint">没有匹配的服务商</p>
          ) : (
            visible.map((provider, index) => {
              const meta = rowMeta(provider.id, provider.enabled);
              return (
              <div
                key={provider.id}
                data-prow-id={provider.id}
                className={`s5-prow${selected === provider.id ? " on" : ""}${
                  provider.enabled ? "" : " off"
                }${meta.problem ? " problem" : ""}`}
                onClick={() => {
                  setSelectedWrapped(provider.id);
                }}
              >
                <span
                  className="s5-prow-icon"
                  style={{
                    ["--c" as string]: getProviderIcon(provider.id).brandColor,
                  }}
                >
                  <ProviderIcon providerId={provider.id} size={28} />
                </span>
                <div>
                  <div className="s5-pname">{provider.name}</div>
                  <div className="s5-psub">{meta.sub}</div>
                </div>
                <span className="s5-pmetric">{meta.metric}</span>
                <V5Toggle
                  on={provider.enabled}
                  disabled={saving}
                  label={`启用 ${provider.name}`}
                  onChange={(next) => toggleEnabled(provider.id, next)}
                />
              </div>
              );
            })
          )}
        </div>
      </div>

      <div className="s5-pdetail">
        {selected === COOKIE_IMPORT_ID ? (
          <>
            <div className="s5-pd-head">
              <div>
                <h3>批量导入浏览器 Cookie</h3>
                <p className="s5-pd-src">
                  从浏览器导出 cookies.txt 或 JSON，一次写入多家。这不是登录。
                </p>
              </div>
              {cookiePhase === "done" ? (
                <div className="s5-actions s5-import-head">
                  <button
                    type="button"
                    className="s5-primary"
                    onClick={() => {
                      const id = cookieWritten[0]?.id;
                      if (id) setSelectedWrapped(id);
                    }}
                  >
                    查看 {cookieWritten[0]?.name ?? "服务商"}
                  </button>
                  <button type="button" className="s5-ghost" onClick={resetCookieFile}>
                    再导入一份
                  </button>
                </div>
              ) : cookiePhase === "confirm" && cookiePreview ? (
                <div className="s5-actions s5-import-head">
                  <button
                    type="button"
                    className="s5-primary"
                    onClick={() => {
                      const ids = cookiePreview.providers
                        .map((entry) => entry.providerId)
                        .filter((id) => cookieTargets[id] !== false);
                      if (ids.length === 0) return;
                      commitCookieImport(ids);
                    }}
                  >
                    确认覆盖并写入
                  </button>
                  <button
                    type="button"
                    className="s5-ghost"
                    onClick={() => setCookiePhase("preview")}
                  >
                    返回
                  </button>
                </div>
              ) : cookieFile && cookiePreview ? (
                <div className="s5-actions s5-import-head">
                  {cookiePreview.providers.length > 0 ? (
                    <button
                      type="button"
                      className="s5-primary"
                      disabled={
                        cookiePreview.providers.every(
                          (entry) => cookieTargets[entry.providerId] === false,
                        )
                      }
                      onClick={() => {
                        const picked = cookiePreview.providers.filter(
                          (entry) => cookieTargets[entry.providerId] !== false,
                        );
                        if (picked.length === 0) return;
                        if (picked.some((entry) => savedCookieIds.has(entry.providerId))) {
                          setCookiePhase("confirm");
                          return;
                        }
                        commitCookieImport(picked.map((entry) => entry.providerId));
                      }}
                    >
                      {(() => {
                        const picked = cookiePreview.providers.filter(
                          (entry) => cookieTargets[entry.providerId] !== false,
                        );
                        const overwrite = picked.filter((entry) =>
                          savedCookieIds.has(entry.providerId),
                        ).length;
                        return overwrite > 0
                          ? `覆盖 ${overwrite} 家并写入`
                          : `写入 ${picked.length} 家`;
                      })()}
                    </button>
                  ) : null}
                  <button type="button" className="s5-ghost" onClick={resetCookieFile}>
                    清除文件
                  </button>
                </div>
              ) : null}
            </div>

            {cookiePhase === "done" ? (
              <div className="s5-import-sheet">
                <p className="s5-import-done">
                  已写入 {cookieWritten.map((item) => item.name).join("、")}。
                </p>
                <p className="s5-pd-lead">回托盘刷新后生效。</p>
              </div>
            ) : !cookieFile || !cookiePreview ? (
            <div className="s5-import-sheet">
                <input
                  ref={cookieInputRef}
                  type="file"
                  accept=".txt,.json,text/plain,application/json"
                  style={{ display: "none" }}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    event.target.value = "";
                    if (!file) return;
                    void loadCookieFile(file);
                  }}
                />
              <button
                type="button"
                className={`s5-dropzone${cookieDragOver ? " is-drag" : ""}`}
                onClick={() => cookieInputRef.current?.click()}
                onDragOver={(event) => {
                  event.preventDefault();
                  setCookieDragOver(true);
                }}
                onDragLeave={() => setCookieDragOver(false)}
                onDrop={(event) => {
                  event.preventDefault();
                  setCookieDragOver(false);
                  const file = event.dataTransfer.files?.[0];
                  if (file) void loadCookieFile(file);
                }}
              >
                {cookiePreviewBusy ? (
                  "正在解析文件…"
                ) : (
                  <>
                    <strong>点击选择 Cookie 文件</strong>
                    <span>把文件拖到这里，或点击选择</span>
                  </>
                )}
              </button>
              <details
                className="s5-import-how"
                open={stepsOpen}
                onToggle={(event) => setStepsOpen(event.currentTarget.open)}
              >
                <summary>查看导出步骤</summary>
                <ol className="s5-import-steps">
                  <li>
                    <span>
                      在浏览器登录后，用 Cookie 导出扩展（推荐 Get cookies.txt LOCALLY）导出{" "}
                      <b>cookies.txt</b> 或 JSON 文件。
                    </span>
                  </li>
                  <li>
                    <span>把文件拖进这里，或点击选择。</span>
                  </li>
                  <li>
                    <span>勾选要写入的服务商。已有 Cookie 的会先确认再覆盖。</span>
                  </li>
                </ol>
              </details>
              <p className="s5-import-privacy">
                支持 Netscape cookies.txt / JSON，不超过 2 MB。文件只在导入时读取，不留原文。
              </p>
              {cookiePreviewError ? (
                <p className="s5-issue" style={{ marginTop: 10, marginBottom: 0 }}>
                  {cookiePreviewError}
                </p>
              ) : null}
            </div>
            ) : cookiePreview.providers.length === 0 ? (
                <div className="s5-import-sheet">
                  <div className="s5-import-file">
                    <span className="s5-import-file-name">{cookieFileName}</span>
                    <span className="s5-import-file-meta">{formatFileSize(cookieFileSize)}</span>
                    <button type="button" className="s5-ghost" onClick={resetCookieFile}>
                      更换
                    </button>
                  </div>
                  <h4 className="s5-import-empty-title">没有匹配的服务商</h4>
                  <p className="s5-pd-lead">
                    这份文件里没有能匹配到任何服务商的 Cookie。请确认导出的是已登录站点的
                    cookies.txt 或 JSON。
                  </p>
                </div>
              ) : (
                <div className="s5-import-sheet">
                  <div className="s5-import-file">
                    <span className="s5-import-file-name">{cookieFileName}</span>
                    <span className="s5-import-file-meta">{formatFileSize(cookieFileSize)}</span>
                    <button type="button" className="s5-ghost" onClick={resetCookieFile}>
                      更换
                    </button>
                  </div>
                  <div className="s5-import-sum">
                    <p>
                      {(() => {
                        const picked = cookiePreview.providers.filter(
                          (entry) => cookieTargets[entry.providerId] !== false,
                        );
                        const overwrite = picked.filter((entry) =>
                          savedCookieIds.has(entry.providerId),
                        ).length;
                        return `写入 ${picked.length} 家 · 覆盖 ${overwrite} · 新增 ${picked.length - overwrite} · 忽略 ${cookiePreview.unmatchedCookieCount} 条`;
                      })()}
                    </p>
                    <button
                      type="button"
                      className="s5-ghost"
                      onClick={() => {
                        const allOn = cookiePreview.providers.every(
                          (entry) => cookieTargets[entry.providerId] !== false,
                        );
                        setCookieTargets(
                          Object.fromEntries(
                            cookiePreview.providers.map((entry) => [entry.providerId, !allOn]),
                          ),
                        );
                        if (allOn) setCookiePhase("preview");
                      }}
                    >
                      {cookiePreview.providers.every(
                        (entry) => cookieTargets[entry.providerId] !== false,
                      )
                        ? "全不选"
                        : "全选"}
                    </button>
                  </div>
                  {cookiePhase === "confirm" ? (
                    <p className="s5-import-note">
                      {cookiePreview.providers
                        .filter(
                          (entry) =>
                            cookieTargets[entry.providerId] !== false &&
                            savedCookieIds.has(entry.providerId),
                        )
                        .map((entry) => catalog.find((item) => item.id === entry.providerId)?.name ?? entry.provider)
                        .join("、")}{" "}
                      已有 Cookie，会换成这份文件里的。
                    </p>
                  ) : null}
                  <div className="s5-import-rows">
                  {cookiePreview.providers.map((entry) => {
                    const row = catalog.find((item) => item.id === entry.providerId);
                    const name = row?.name ?? entry.provider;
                    const on = cookieTargets[entry.providerId] !== false;
                    const warn = savedCookieIds.has(entry.providerId);
                    return (
                      <div
                        className={`s5-import-card${warn ? " is-warn" : ""}`}
                        key={entry.providerId}
                      >
                        <span
                          className="s5-import-ico"
                          style={{
                            ["--c" as string]: getProviderIcon(entry.providerId).brandColor,
                          }}
                        >
                          <ProviderIcon providerId={entry.providerId} size={17} />
                        </span>
                        <div className="s5-import-card-text">
                          <div className="s5-import-name">{name}</div>
                          <div className={`s5-import-help${warn ? " is-warn" : ""}`}>
                            {warn
                              ? "会覆盖已经保存的 Cookie"
                              : "现在没有已保存的 Cookie，会新写入"}
                          </div>
                        </div>
                        <button
                          type="button"
                          className={`s5-import-check${on ? " on" : ""}`}
                          aria-pressed={on}
                          aria-label={`写入 ${name}`}
                          onClick={() => {
                            const next = !on;
                            setCookieTargets((cur) => ({ ...cur, [entry.providerId]: next }));
                            if (!next && warn && cookiePhase === "confirm") {
                              const still = cookiePreview.providers.some((item) => {
                                const checked =
                                  item.providerId === entry.providerId
                                    ? false
                                    : cookieTargets[item.providerId] !== false;
                                return checked && savedCookieIds.has(item.providerId);
                              });
                              if (!still) setCookiePhase("preview");
                            }
                          }}
                        >
                          {on ? <ImportCheckMark /> : null}
                        </button>
                      </div>
                    );
                  })}
                  </div>
                  {(cookiePreview.unmatchedCookieCount > 0 ||
                    (cookiePreview.unmatchedDomains?.length ?? 0) > 0) ? (
                    <details
                      className="s5-import-miss"
                      open={unmatchedOpen}
                      onToggle={(event) => setUnmatchedOpen(event.currentTarget.open)}
                    >
                      <summary>
                        {cookiePreview.unmatchedCookieCount} 条没对上任何服务商
                      </summary>
                      {cookiePreview.unmatchedDomains && cookiePreview.unmatchedDomains.length > 0 ? (
                        <ul>
                          {cookiePreview.unmatchedDomains.map((domain) => (
                            <li key={domain}>{domain}</li>
                          ))}
                        </ul>
                      ) : null}
                    </details>
                  ) : null}
                  <p className="s5-import-privacy">
                    写入后只保存在本机。不会改成密钥，也不会新建账号。
                  </p>
                </div>
              )}
          </>
        ) : selectedProvider ? (
          <>
            <div className="s5-pd-head">
              <div>
                <h3>{selectedProvider.name}</h3>
                <p className="s5-pd-src">{who}</p>
              </div>
              <ProviderActionBar
                providerId={selectedProvider.id}
                motion={motion}
                onError={handleActionError}
                onDetail={handleDetail}
              />
            </div>

            {detail?.lastError ? (
              <div className="s5-issue">
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                  }}
                >
                  <strong>需要先认证</strong>
                  <button
                    type="button"
                    className="s5-ghost"
                    onClick={() => {
                      const text = detail?.lastError;
                      if (!text) return;
                      void navigator.clipboard
                        ?.writeText(text)
                        .then(() => setNote("已复制"));
                    }}
                  >
                    复制
                  </button>
                </div>
                <p>{detail.lastError}</p>
              </div>
            ) : null}

            <ProviderUsageCard
              detail={detail}
              display={quotaDisplayContext(settings, "dashboard")}
              localUsagePeriod={settings.localUsagePeriod ?? "today"}
            />

            <div className="s5-pd-card">
              {/* 第一层级：用量读取方式 —— Cookies 与 API 密钥等获取方式在此
                  选择，下方的管理内容跟随此选择变化。 */}
              <div className="s5-pd-l1">
                {(() => {
                  const sources = authCaps?.[selectedProvider.id]?.availableSources ?? [];
                  const capsNow = authCaps?.[selectedProvider.id] ?? null;
                  const supportsApiKey = capsNow?.supportsApiKey ?? false;
                  const loginFlow = capsNow?.loginFlow ?? null;
                  const l1Options: { value: string; label: string }[] = [
                    { value: "auto", label: "自动选择" },
                  ];
                  if (cookieSourceOptions.length > 0)
                    l1Options.push({ value: "cookies", label: "Cookies" });
                  if (supportsApiKey && sources.includes("oauth"))
                    l1Options.push({ value: "apikey", label: "API 密钥" });
                  else if (sources.includes("oauth"))
                    l1Options.push({ value: "oauth", label: "OAuth 授权" });
                  if (sources.includes("cli") || loginFlow)
                    l1Options.push({ value: "cli", label: "CLI 登录" });
                  const usage = detail?.usageSource?.trim() || "auto";
                  const currentRaw =
                    usage === "web"
                      ? "cookies"
                      : usage === "oauth"
                        ? supportsApiKey
                          ? "apikey"
                          : "oauth"
                        : usage === "cli"
                          ? "cli"
                          : "auto";
                  const currentLabel =
                    l1Options.find((option) => option.value === currentRaw)?.label ??
                    "自动选择";
                  return (
                    <div className="s5-pd-row">
                      <div>
                        <div className="s5-field-label">用量读取方式</div>
                        <div className="s5-field-help">
                          当前：{currentLabel} —— Cookies 与 API 密钥在这里切换；下方的管理内容跟随此选择。
                        </div>
                      </div>
                      <V5Select
                        value={currentRaw}
                        options={l1Options}
                        onChange={(value) => {
                          const persisted =
                            value === "cookies"
                              ? "web"
                              : value === "apikey" || value === "oauth"
                                ? "oauth"
                                : value === "cli"
                                  ? "cli"
                                  : "auto";
                          void requireActionResult(
                            (actionDispatcher as ActionDispatcher).dispatch({
                              type: "setUsageSource",
                              target: { kind: "provider", providerId: selectedProvider.id },
                              source: persisted,
                            }),
                          )
                            .then(() => getProviderDetail(selectedProvider.id))
                            .then((next) => {
                              handleDetail(next);
                              setNote("已更新用量读取方式");
                            })
                            .catch((error) => setNote(String(error)));
                        }}
                      />
                    </div>
                  );
                })()}
              </div>
              <div className="s5-pd-sub">
                {(() => {
                  if (!selectedProvider) return null;
                  const caps = authCaps?.[selectedProvider.id] ?? null;
                  const loginFlow = caps?.loginFlow ?? null;
                  const supportsApiKey = caps?.supportsApiKey ?? false;
                  const hasCookies = cookieSourceOptions.length > 0;
                  const authBody = (authMethod: AuthMethod) => (
                    <AuthBody
                      provider={selectedProvider}
                      method={authMethod}
                      hasApiKey={selectedHasApiKey}
                      hasCookie={selectedHasCookie}
                      isLoggedIn={selectedIsLoggedIn}
                      loginFlow={loginFlow}
                      hasLoginLadder={
                        Boolean(loginFlow) ||
                        (caps?.availableSources ?? []).some(
                          (value) => value === "cli" || value === "oauth",
                        )
                      }
                      cookieSource={cookieSource}
                      cookieSourceOptions={cookieSourceOptions}
                      onCookieSourceChanged={(value) => setCookieSource(value)}
                      onNote={setNote}
                      onConfirm={(text, action) => setConfirm({ text, action })}
                      onCredentialChange={() => void reloadCredentials()}
                    />
                  );
                  const usage = detail?.usageSource?.trim() || "auto";
                  const l1 =
                    usage === "web"
                      ? "cookies"
                      : usage === "oauth"
                        ? supportsApiKey
                          ? "apikey"
                          : "oauth"
                        : usage === "cli"
                          ? "cli"
                          : "auto";
                  // Cookies 选中：内部再区分 浏览器 / 已保存(导入)。
                  if (l1 === "cookies") {
                    const cookieSub = cookieSourceOptions
                      .filter((option) => option.value !== "off")
                      .map((option) => ({
                        value: option.value === "auto" ? "browser" : "saved",
                        label: option.label,
                      }));
                    const cookieValue = cookieSource === "auto" ? "browser" : "saved";
                    return (
                      <>
                        <div className="s5-pd-row">
                          <div>
                            <div className="s5-field-label">Cookie 来源</div>
                            <div className="s5-field-help">
                              Cookies 内部再区分：浏览器读取，或你导入/粘贴的已保存 Cookie。
                            </div>
                          </div>
                          <V5Select
                            value={cookieValue}
                            options={cookieSub}
                            onChange={(value) => {
                              const source = value === "browser" ? "auto" : "manual";
                              void requireActionResult(
                                (actionDispatcher as ActionDispatcher).dispatch({
                                  type: "setCookieSource",
                                  target: { kind: "provider", providerId: selectedProvider.id },
                                  source,
                                }),
                              )
                                .then(() => {
                                  setCookieSource(source);
                                  setNote("已更新 Cookie 来源");
                                })
                                .catch((error) => setNote(String(error)));
                            }}
                          />
                        </div>
                        {authBody("cookie")}
                      </>
                    );
                  }
                  if (l1 === "apikey") return authBody("apiKey");
                  if (l1 === "oauth" || l1 === "cli") {
                    return loginFlow ? (
                      authBody("signIn")
                    ) : (
                      <p className="s5-pd-lead">
                        该服务商没有提供对应的登录入口；请把用量读取方式切回「自动选择」。
                      </p>
                    );
                  }
                  // 自动选择：由服务商按能力自动选择通道，用户没有需要
                  // 管理的东西——下方什么都不显示（2026-09-06 用户指定）。
                  return null;
                })()}
              </div>
            </div>

            <TokenCard
              providerId={selectedProvider.id}
              hasToken={tokenAccountIds?.includes(selectedProvider.id) ?? false}
              onConfirm={(text, action) => setConfirm({ text, action })}
            />

            <div className="s5-pd-card">
              <div className="s5-pd-card-h">
                <h4>显示与凭据</h4>
              </div>
              <div className="s5-pd-row">
                <div>
                  <div className="s5-field-label">托盘小图标</div>
                  <div className="s5-field-help">
                    只影响通知区那个点，不是卡片密度
                  </div>
                </div>
                <V5Select
                  value={settings.providerMetrics[selectedProvider.id] ?? "automatic"}
                  disabled={saving}
                  options={[
                    { value: "automatic", label: "自动" },
                    { value: "session", label: "5h" },
                    { value: "weekly", label: "周" },
                  ]}
                  onChange={(value) =>
                    set({
                      providerMetrics: {
                        ...settings.providerMetrics,
                        [selectedProvider.id]: value as MetricPreference,
                      },
                    })
                  }
                />
              </div>
              {regionOptions.length > 0 ? (
                <div className="s5-pd-row">
                  <div>
                    <div className="s5-field-label">区域</div>
                    {regionError ? (
                      <div className="s5-field-help">{regionError}</div>
                    ) : null}
                  </div>
                  <V5Select
                    value={regionValue || regionOptions[0]?.value || ""}
                    options={regionOptions}
                    disabled={saving || regionBusy}
                    onChange={(value) => {
                      setRegionBusy(true);
                      setRegionError(null);
                      void requireActionResult(
                        (actionDispatcher as ActionDispatcher).dispatch({
                          type: "setRegion",
                          target: { kind: "provider", providerId: selectedProvider.id },
                          region: value,
                        }),
                      )
                        .then(() => setRegionValue(value))
                        .catch((error) => setRegionError(String(error)))
                        .finally(() => setRegionBusy(false));
                    }}
                  />
                </div>
              ) : null}
              <div className="s5-pd-row">
                <div>
                  <div className="s5-field-label">凭据存储</div>
                  <div className="s5-field-help">
                    API 密钥、已保存 Cookie 和令牌保存在本机受保护存储；已保存的内容在上方对应卡片里管理
                  </div>
                </div>
                <button
                  type="button"
                  className="s5-ghost danger"
                  onClick={() =>
                    setConfirm({
                      text: `撤销 ${selectedProvider.name} 的本地凭据。不会删除账号本身。`,
                      action: () => {
                        void requireActionResult(
                          (actionDispatcher as ActionDispatcher).dispatch({
                            type: "revokeCredentials",
                            target: { kind: "provider", providerId: selectedProvider.id },
                          }),
                        )
                          .then(() => setNote(null))
                          .catch((error) => setNote(String(error)));
                      },
                    })
                  }
                >
                  撤销
                </button>
              </div>
            </div>
          </>
        ) : (
          <div className="s5-hint">未选择服务商</div>
        )}
        {note ? <p className="s5-hint">{note}</p> : null}
      </div>

      {confirm ? (
        <ConfirmDialog
          text={confirm.text}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            confirm.action();
            setConfirm(null);
          }}
        />
      ) : null}
    </div>
  );
}
