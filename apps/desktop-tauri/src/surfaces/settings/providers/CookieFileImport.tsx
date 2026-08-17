import { useRef, useState, type DragEvent } from "react";
import { importCookieFile, previewCookieFile } from "../../../lib/tauri";
import type { CookieFilePreviewBridge } from "../../../types/bridge";

/** Sentinel "provider id" for the pinned sidebar row that selects this pane
 * instead of a real provider — see `ProvidersSidebar`/`ProvidersTab`. */
export const COOKIE_IMPORT_ID = "__cookie_import__";

/** Detail-pane view for importing a user-exported browser Cookie file,
 * reached by selecting the pinned "批量导入 Cookie" row in the sidebar. */
export function CookieFileImport() {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [contents, setContents] = useState<string | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const [preview, setPreview] = useState<CookieFilePreviewBridge | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dragActive, setDragActive] = useState(false);

  const chooseFile = async (file: File | undefined) => {
    if (!file) return;
    setError(null);
    setPreview(null);
    setContents(null);
    if (file.size > 2 * 1024 * 1024) {
      setError("Cookie 文件不能超过 2 MB。");
      return;
    }
    try {
      const nextContents = await file.text();
      const nextPreview = await previewCookieFile(nextContents);
      setContents(nextContents);
      setFileName(file.name);
      setPreview(nextPreview);
      setSelected(nextPreview.providers.map((provider) => provider.providerId));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const resetSelection = () => {
    setContents(null);
    setFileName(null);
    setPreview(null);
    setSelected([]);
    setError(null);
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragActive(false);
    void chooseFile(event.dataTransfer.files?.[0]);
  };

  const importSelected = async () => {
    if (!contents || selected.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      await importCookieFile(contents, selected);
      resetSelection();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="provider-detail cookie-import-pane">
      <div className="cookie-import-pane__header">
        <span className="cookie-import-pane__header-icon" aria-hidden>
          <svg width="20" height="20" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.3} strokeLinecap="round" strokeLinejoin="round">
            <path d="M8 10.5V2.5M8 2.5 5.2 5.3M8 2.5l2.8 2.8" />
            <path d="M2.5 10v2.3a.7.7 0 0 0 .7.7h9.6a.7.7 0 0 0 .7-.7V10" />
          </svg>
        </span>
        <div>
          <h3>批量导入网页会话</h3>
          <p>
            {/* TODO(lane-s-i18n) */}
            不是第四种登录。和每家里的「捕获 / 粘贴」一样，都是网页会话，只是一次写给好几家。
          </p>
        </div>
      </div>

      <input
        ref={fileInputRef}
        className="cookie-import-pane__input"
        type="file"
        accept=".txt,.json,text/plain,application/json"
        onChange={(event) => void chooseFile(event.target.files?.[0])}
      />

      {fileName ? (
        <div className="cookie-import-pane__summary">
          <span className="cookie-import-pane__summary-icon" aria-hidden>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round">
              <path d="M4 1.8h5.4L12 4.4v9.3a.5.5 0 0 1-.5.5h-7a.5.5 0 0 1-.5-.5V2.3a.5.5 0 0 1 .5-.5Z" />
              <path d="M9.2 1.8v2.6H12" />
            </svg>
          </span>
          <span className="cookie-import-pane__summary-text" title={fileName}>
            {fileName}
            {preview && preview.providers.length > 0
              ? ` · 已识别 ${preview.providers.length} 个服务商`
              : ""}
          </span>
          <button
            type="button"
            className="cookie-import-pane__change"
            disabled={busy}
            onClick={resetSelection}
          >
            更换
          </button>
        </div>
      ) : (
        <div
          className={`cookie-import-pane__dropzone${dragActive ? " cookie-import-pane__dropzone--active" : ""}`}
          role="button"
          tabIndex={0}
          onClick={() => fileInputRef.current?.click()}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              fileInputRef.current?.click();
            }
          }}
          onDragOver={(event) => {
            event.preventDefault();
            setDragActive(true);
          }}
          onDragLeave={() => setDragActive(false)}
          onDrop={handleDrop}
        >
          <svg width="26" height="26" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.2} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d="M8 10.5V2.5M8 2.5 5.2 5.3M8 2.5l2.8 2.8" />
            <path d="M2.5 10v2.3a.7.7 0 0 0 .7.7h9.6a.7.7 0 0 0 .7-.7V10" />
          </svg>
          <strong>点击选择 Cookie 文件</strong>
          <span>或将文件拖拽到此处</span>
        </div>
      )}

      {error && <p className="cookie-import-pane__error">{error}</p>}

      {preview && (
        <div className="cookie-import-pane__preview">
          {preview.providers.length === 0 ? (
            <span className="cookie-import-pane__muted">未识别到当前支持服务商的 Cookie。</span>
          ) : (
            <>
              <div className="cookie-import-pane__providers">
                {preview.providers.map((provider) => (
                  <label key={provider.providerId} className="cookie-import-pane__provider">
                    <input
                      type="checkbox"
                      className="toggle toggle--sm"
                      checked={selected.includes(provider.providerId)}
                      disabled={busy}
                      onChange={(event) =>
                        setSelected((current) =>
                          event.target.checked
                            ? [...current, provider.providerId]
                            : current.filter((id) => id !== provider.providerId),
                        )
                      }
                    />
                    <span className="cookie-import-pane__provider-name">{provider.provider}</span>
                    <span className="cookie-import-pane__provider-count">{provider.cookieCount}</span>
                  </label>
                ))}
              </div>
              <div className="cookie-import-pane__footer">
                {preview.unmatchedCookieCount > 0 && (
                  <span className="cookie-import-pane__muted">
                    {preview.unmatchedCookieCount} 个不匹配 Cookie 将被跳过
                  </span>
                )}
                <button
                  type="button"
                  className="credential-btn credential-btn--primary"
                  disabled={busy || selected.length === 0}
                  onClick={() => void importSelected()}
                >
                  导入所选服务商
                </button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
