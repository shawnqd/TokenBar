import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Sortable from "sortablejs";
import { ProviderIcon } from "../../../components/providers/ProviderIcon";
import { getProviderIcon } from "../../../components/providers/providerIcons";
import { useProviders } from "../../../hooks/useProviders";
import { getProviderBalance } from "../../../lib/providerBalance";
import {
  primaryQuotaState,
  quotaDisplayContext,
  quotaPercentContext,
  quotaPercentDisplay,
} from "../../../lib/quotaDisplay";
import {
  captureProviderLogin,
  closeProviderLogin,
  openProviderLogin,
  removeManualCookie,
  reorderProviders,
  revokeProviderCredentials,
  setManualCookie,
  setProviderCookieSource,
  setApiKey,
  removeApiKey,
  triggerProviderLogin,
  getApiKeys,
  getGeminiCliSignedIn,
  getManualCookies,
  importCookieFile,
} from "../../../lib/tauri";
import type {
  MetricPreference,
  ProviderDetail,
  ApiKeyInfoBridge,
  CookieInfoBridge,
} from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import ProviderActionBar from "./ProviderActionBar";
import ProviderUsageCard from "./ProviderUsageCard";
import { TokenAccountsPanel } from "../tokens/TokenAccountsPanel";
import {
  COOKIE_IMPORT_ID,
  COOKIE_IMPORT_TARGETS,
  METHOD_LABEL,
  buildProviderCatalog,
  type AuthMethod,
  type FixtureProvider,
} from "./htmlFixture";
import { ConfirmDialog, V5Seg, V5Select, V5Toggle } from "./v5Controls";

function listUpdatedLabel(updatedMs: number, nowMs: number): string {
  const diffSecs = Math.max(0, Math.floor((nowMs - updatedMs) / 1000));
  if (diffSecs < 60) return "刚刚更新";
  const diffMins = Math.floor(diffSecs / 60);
  if (diffMins < 60) return `${diffMins} 分钟前`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours} 小时前`;
  return `${Math.floor(diffHours / 24)} 天前`;
}

function AuthBody({
  provider,
  method,
  loginOpen,
  hasApiKey,
  hasCookie,
  isLoggedIn,
  onLoginOpen,
  onNote,
  onConfirm,
  onCredentialChange,
}: {
  provider: FixtureProvider;
  method: AuthMethod;
  loginOpen: boolean;
  hasApiKey: boolean;
  hasCookie: boolean;
  isLoggedIn: boolean;
  onLoginOpen: (open: boolean) => void;
  onNote: (text: string) => void;
  onConfirm: (text: string, action: () => void) => void;
  onCredentialChange: () => void;
}) {
  const [cookieText, setCookieText] = useState("");
  const [keyLabel, setKeyLabel] = useState("");
  const [keyValue, setKeyValue] = useState("");

  if (method === "cookie") {
    return (
      <div className="s5-login-hero">
        <p>
          在网页里登录 {provider.name}，回到这里点「捕获」。不用自己复制密码。
        </p>
        {hasCookie ? <div className="s5-login-ok">已保存会话</div> : null}
        <div className="s5-actions">
          <button
            type="button"
            className="s5-primary"
            onClick={() => {
              onLoginOpen(true);
              void openProviderLogin(provider.id)
                .then(() => onNote("已打开登录窗"))
                .catch((error) => onNote(String(error)));
            }}
          >
            {loginOpen ? "登录窗已打开" : "打开网页登录"}
          </button>
          {loginOpen ? (
            <>
              <button
                type="button"
                className="s5-ghost"
                onClick={() =>
                  void captureProviderLogin(provider.id)
                    .then(() => {
                      onNote("已捕获网页会话");
                      onCredentialChange();
                    })
                    .catch((error) => onNote(String(error)))
                }
              >
                我已登录，捕获
              </button>
              <button
                type="button"
                className="s5-ghost"
                onClick={() => {
                  onLoginOpen(false);
                  void closeProviderLogin().catch(() => {});
                }}
              >
                关闭
              </button>
            </>
          ) : null}
        </div>
        <details className="s5-adv">
          <summary>已经登录好了？粘贴或批量导入</summary>
          <p className="s5-pd-lead">
            {hasCookie
              ? "已有一份。新的会覆盖旧的。"
              : "粘贴 Cookie，或用左边「批量导入网页会话」一次写给好几家。都算你自己提供的会话，和打开网页捕获是同一条路。"}
          </p>
          <textarea
            className="s5-area"
            placeholder="粘贴后不会回显"
            value={cookieText}
            onChange={(event) => setCookieText(event.target.value)}
          />
          <div className="s5-actions">
            <button
              type="button"
              className="s5-ghost"
              onClick={() =>
                void setManualCookie(provider.id, cookieText.trim())
                  .then(() => {
                    setCookieText("");
                    onNote("已保存网页会话");
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
                    void removeManualCookie(provider.id)
                      .then(() => {
                        onNote("已删除网页会话");
                        onCredentialChange();
                      })
                      .catch((error) => onNote(String(error))),
                  )
                }
              >
                删除已保存的会话
              </button>
            ) : null}
          </div>
        </details>
        {provider.cookieSource ? (
          <details className="s5-adv">
            <summary>高级：刷新时从哪读会话</summary>
            <p className="s5-pd-lead">
              一般不用改。自动会先用你导入、粘贴或捕获的那份，没有再去浏览器里找。
            </p>
            <V5Seg
              value="auto"
              options={[
                { value: "auto", label: "自动" },
                { value: "manual", label: "只用我保存的" },
              ]}
              onChange={(value) =>
                void setProviderCookieSource(provider.id, value)
                  .then(() => onNote("已更新会话读取位置"))
                  .catch((error) => onNote(String(error)))
              }
            />
          </details>
        ) : null}
      </div>
    );
  }

  if (method === "signIn") {
    const gemini = provider.id === "gemini";
    return (
      <div className="s5-login-hero">
        <p>
          {gemini
            ? "如果本机已经用 Gemini / Antigravity 登录过，点检测即可，不用填密钥。"
            : "用系统浏览器完成登录。不要在这里粘贴密钥或 Cookie。"}
        </p>
        {isLoggedIn ? <div className="s5-login-ok">已登录</div> : null}
        <div className="s5-actions">
          <button
            type="button"
            className="s5-primary"
            onClick={() =>
              void (gemini ? getGeminiCliSignedIn() : triggerProviderLogin(provider.id))
                .then(() => onNote(gemini ? "检测完成" : "已发起登录"))
                .catch((error) => onNote(String(error)))
            }
          >
            {gemini
              ? "检测本机登录"
              : isLoggedIn
                ? "切换账号"
                : "打开登录"}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="s5-login-hero">
      <p>这个服务用密钥。填一次并保存，之后会自动刷新用量。</p>
      {hasApiKey ? <div className="s5-login-ok">密钥已保存</div> : null}
      {!hasApiKey ? <p className="s5-pd-lead">还没有保存密钥。</p> : null}
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
          <div className="s5-field-label">密钥</div>
          <input
            className="s5-textin"
            type="password"
            value={keyValue}
            onChange={(event) => setKeyValue(event.target.value)}
            placeholder="不会回显"
          />
        </div>
        <div className="s5-actions">
          <button
            type="button"
            className="s5-primary"
            onClick={() =>
              void setApiKey(provider.id, keyValue.trim(), keyLabel.trim() || undefined)
                .then(() => {
                  setKeyValue("");
                  setKeyLabel("");
                  onNote("已保存密钥");
                  onCredentialChange();
                })
                .catch((error) => onNote(String(error)))
            }
          >
            保存
          </button>
          {hasApiKey ? (
            <button
              type="button"
              className="s5-ghost danger"
              onClick={() =>
                onConfirm(`删除 ${provider.name} 的密钥，不影响其他服务商。`, () =>
                  void removeApiKey(provider.id)
                    .then(() => {
                      onNote("已删除密钥");
                      onCredentialChange();
                    })
                    .catch((error) => onNote(String(error))),
                )
              }
            >
              删除
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function TokenCard({ provider }: { provider: FixtureProvider }) {
  if (!provider.hasToken) return null;

  return (
    <div className="s5-pd-card">
      <div className="s5-pd-card-h">
        <h4>账号</h4>
      </div>
      <TokenAccountsPanel providerId={provider.id} compact />
    </div>
  );
}

export default function ProvidersPage({
  settings,
  set,
  saving,
  catalog: liveCatalog,
}: SettingsPageProps) {
  const enabled = settings.enabledProviders ?? [];
  const motion = settings.enableAnimations !== false;
  const { providers: snapshots, refresh } = useProviders({
    refreshOnMount: true,
  });
  const [nowMs, setNowMs] = useState(() => Date.now());
  const snapshotById = useMemo(
    () => new Map(snapshots.map((row) => [row.providerId, row])),
    [snapshots],
  );

  useEffect(() => {
    const timer = window.setInterval(() => {
      setNowMs(Date.now());
      refresh();
    }, 60_000);
    return () => window.clearInterval(timer);
  }, [refresh]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(
    () => enabled[0] ?? liveCatalog?.[0]?.id ?? "claude",
  );
  const [authById, setAuthById] = useState<Record<string, AuthMethod>>({});
  const [loginOpen, setLoginOpen] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{
    text: string;
    action: () => void;
  } | null>(null);
  const [cookieFile, setCookieFile] = useState(false);
  const [cookieFileContents, setCookieFileContents] = useState("");
  const [cookieFileName, setCookieFileName] = useState("");
  const cookieInputRef = useRef<HTMLInputElement>(null);
  const [cookieTargets, setCookieTargets] = useState<Record<string, boolean>>(
    () =>
      Object.fromEntries(COOKIE_IMPORT_TARGETS.map((id) => [id, true])),
  );
  const [apiKeys, setApiKeys] = useState<ApiKeyInfoBridge[]>([]);
    const [manualCookies, setManualCookies] = useState<CookieInfoBridge[]>([]);
  const [detail, setDetail] = useState<ProviderDetail | null>(null);

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
      ),
    [enabled, liveCatalog, settings.providerOrder],
  );

  const visible = catalog.filter((p) => {
    if (!query.trim()) return true;
    const q = query.trim().toLowerCase();
    return p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q);
  });

    const rowsRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
      const el = rowsRef.current;
      if (!el || saving) return;
      // 用成熟排序库 sortablejs，不自研动画：跟手 + 让位 + 归位全靠库。
      const sortable = Sortable.create(el, {
        animation: 150,
        draggable: ".s5-prow",
        ghostClass: "s5-prow--ghost",
        chosenClass: "s5-prow--dragging",
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
          void reorderProviders(nextIds).catch(() => setNote("排序保存失败"));
        },
      });
      return () => sortable.destroy();
    }, [saving, catalog]);

  const selectedProvider = catalog.find((p) => p.id === selected);
  const method =
    (selectedProvider &&
      (authById[selectedProvider.id] ?? selectedProvider.primary)) ||
    "cookie";

    const selectedHasApiKey = selectedProvider
      ? apiKeys.some((key) => key.providerId === selectedProvider.id)
      : false;
    const selectedHasCookie = selectedProvider
      ? manualCookies.some((cookie) => cookie.providerId === selectedProvider.id)
      : false;
    const selectedSnapshot = selectedProvider
      ? snapshotById.get(selectedProvider.id)
      : undefined;
    const selectedIsLoggedIn = Boolean(
      selectedSnapshot && !selectedSnapshot.error,
    );
    const localCredentialLabel = selectedHasApiKey
      ? "已保存 API 密钥"
      : selectedHasCookie
        ? "已保存网页会话"
        : selectedIsLoggedIn
          ? "已检测到登录状态"
          : "还没有本地凭据";

  const toggleEnabled = (id: string, next: boolean) => {
    const current = new Set(enabled);
    if (next) current.add(id);
    else current.delete(id);
    set({ enabledProviders: [...current] });
  };

  const rowMeta = (id: string, isOn: boolean) => {
    const snap = snapshotById.get(id) ?? null;
    if (!isOn) return { sub: "未配置", metric: "", problem: false };
    if (!snap || snap.error) {
      return { sub: "未配置", metric: "", problem: Boolean(snap?.error) };
    }
    const updatedMs = new Date(snap.updatedAt).getTime();
    const time = Number.isFinite(updatedMs)
      ? listUpdatedLabel(updatedMs, nowMs)
      : "刚刚更新";
    let metric = "";
    if (primaryQuotaState(snap) === "quota" && snap.primary && Number.isFinite(snap.primary.usedPercent)) {
      metric = `${quotaPercentDisplay(snap.primary, quotaPercentContext(settings, "dashboard")).rounded}%`;
    } else {
      const balance = getProviderBalance(snap).balance;
      if (balance?.kind === "balance" && !balance.unavailable) {
        metric = balance.amount;
      }
    }
    const stale =
      Number.isFinite(updatedMs) && nowMs - updatedMs > 10 * 60_000;
    return { sub: time, metric, problem: stale };
  };

  const who = selectedProvider
    ? [
        detail?.plan ?? selectedProvider.plan,
        settings.hidePersonalInfo
          ? null
          : (detail?.email ?? selectedProvider.email),
      ]
        .filter(Boolean)
        .join(" · ") ||
      (selectedProvider.enabled ? METHOD_LABEL[method] : "未登录")
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
        <button
          type="button"
          className={`s5-pinned${selected === COOKIE_IMPORT_ID ? " on" : ""}`}
          onClick={() => setSelected(COOKIE_IMPORT_ID)}
        >
          批量导入网页会话
        </button>
        <div ref={rowsRef} className="s5-prov-rows">
          {visible.length === 0 ? (
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
                  setSelected(provider.id);
                  setLoginOpen(false);
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
                <h3>批量导入网页会话</h3>
                <p className="s5-pd-src">
                  不是第四种登录。和每家里的「捕获 / 粘贴」一样，都是网页会话，只是一次写给好几家。
                </p>
              </div>
            </div>
            <div className="s5-pd-card">
              <p className="s5-pd-lead" style={{ margin: "0 0 10px" }}>
                从浏览器导出 Cookie 文件（不超过 2 MB）。这里不显示原文。导入后刷新会先用这份；高级里不用改，保持「自动」即可。
              </p>
                <input
                  ref={cookieInputRef}
                  type="file"
                  accept=".txt,.json,text/plain,application/json"
                  style={{ display: "none" }}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (!file) return;
                    void file.text().then((text) => {
                      setCookieFileContents(text);
                      setCookieFileName(file.name);
                      setCookieFile(true);
                    });
                  }}
                />
              <button
                type="button"
                className="s5-dropzone"
                onClick={() => cookieInputRef.current?.click()}
              >
                {cookieFileName ? `已选择：${cookieFileName}` : "把文件拖到这里，或点击选择"}
              </button>
            </div>
            {cookieFile ? (
              <div className="s5-pd-card">
                <div className="s5-pd-card-h">
                  <h4>将作为网页会话写入</h4>
                </div>
                <p className="s5-pd-lead">
                  勾选的服务商会换成这份会话。不会改成密钥，也不会新建账号。
                </p>
                {COOKIE_IMPORT_TARGETS.map((id) => {
                  const row = catalog.find((item) => item.id === id);
                  return (
                    <div className="s5-pd-row" key={id}>
                      <div className="s5-field-label">{row?.name ?? id}</div>
                      <V5Toggle
                        on={Boolean(cookieTargets[id])}
                        onChange={(next) =>
                          setCookieTargets((cur) => ({ ...cur, [id]: next }))
                        }
                      />
                    </div>
                  );
                })}
                <div className="s5-actions" style={{ marginTop: 12 }}>
                  <button
                    type="button"
                    className="s5-primary"
                    onClick={() => {
                      const ids = COOKIE_IMPORT_TARGETS.filter((id) => cookieTargets[id]);
                      void importCookieFile(cookieFileContents, ids)
                        .then(() => {
                          setCookieFile(false);
                          setCookieFileContents("");
                          setCookieFileName("");
                          setNote(`已写入 ${ids.length} 个服务商的网页会话`);
                        })
                        .catch((error) => setNote(String(error)));
                    }}
                  >
                    写入所选
                  </button>
                  <button
                    type="button"
                    className="s5-ghost"
                    onClick={() => setCookieFile(false)}
                  >
                    取消
                  </button>
                </div>
              </div>
            ) : null}
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

            {detail?.lastError || selectedProvider.error ? (
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
                    onClick={() =>
                      void navigator.clipboard
                        ?.writeText(
                          detail?.lastError ?? selectedProvider.error ?? "",
                        )
                        .then(() => setNote("已复制"))
                    }
                  >
                    复制
                  </button>
                </div>
                <p>{detail?.lastError ?? selectedProvider.error}</p>
              </div>
            ) : null}

            <ProviderUsageCard
              detail={detail}
              display={quotaDisplayContext(settings, "dashboard")}
              localUsagePeriod={settings.localUsagePeriod ?? "today"}
            />

            <div className="s5-pd-card">
              <div className="s5-pd-card-h">
                <h4>怎么登录</h4>
                {selectedProvider.methods.length > 1 ? (
                  <V5Seg
                    value={method}
                    options={selectedProvider.methods.map((item) => ({
                      value: item,
                      label: METHOD_LABEL[item],
                    }))}
                    onChange={(value) =>
                      setAuthById((cur) => ({
                        ...cur,
                        [selectedProvider.id]: value as AuthMethod,
                      }))
                    }
                  />
                ) : null}
              </div>
              {selectedProvider.methods.length > 1 ? (
                <p className="s5-pd-lead">
                  选一种就行。改了只换进门方式，下面的账号列表不会丢。
                </p>
              ) : null}
              <AuthBody
                provider={selectedProvider}
                method={method}
                loginOpen={loginOpen}
                  hasApiKey={selectedHasApiKey}
                  hasCookie={selectedHasCookie}
                  isLoggedIn={selectedIsLoggedIn}
                onLoginOpen={setLoginOpen}
                onNote={setNote}
                onConfirm={(text, action) => setConfirm({ text, action })}
                  onCredentialChange={() => void reloadCredentials()}
              />
            </div>

            <TokenCard provider={selectedProvider} />

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
                    { value: "session", label: "会话" },
                    { value: "weekly", label: "周" },
                    ...(selectedProvider.quota === "hybrid"
                      ? [{ value: "extraUsage", label: "额外用量" }]
                      : []),
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
              {selectedProvider.region ? (
                <div className="s5-pd-row">
                  <div className="s5-field-label">区域</div>
                  <V5Select
                    value="美国东部"
                    options={[
                      { value: "美国东部", label: "美国东部" },
                      { value: "美国西部", label: "美国西部" },
                    ]}
                    onChange={() => {}}
                  />
                </div>
              ) : null}
              <div className="s5-pd-row">
                <div>
                  <div className="s5-field-label">本地凭据</div>
                  <div className="s5-field-help">{localCredentialLabel}</div>
                </div>
                <button
                  type="button"
                  className="s5-ghost danger"
                  onClick={() =>
                    setConfirm({
                      text: `撤销 ${selectedProvider.name} 的本地凭据。不会删除账号本身。`,
                      action: () => {
                        void revokeProviderCredentials(selectedProvider.id)
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
