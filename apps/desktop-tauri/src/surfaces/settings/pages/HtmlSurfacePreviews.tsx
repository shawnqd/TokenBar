import type { ReactNode } from "react";

export function HtmlFloatPreview({
  enabled,
  vertical,
  darkText,
  asUsed,
  showReset,
}: {
  enabled: boolean;
  vertical: boolean;
  darkText: boolean;
  asUsed: boolean;
  showReset: boolean;
}) {
  const used = asUsed ? "82%" : "18%";
  const cls = [
    "s5-fbar",
    vertical ? "vert" : "",
    darkText ? "light" : "",
    enabled ? "" : "dim",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cls}>
      <span>Claude</span>
      <b>{used}</b>
      <span>DeepSeek</span>
      <b>¥69</b>
      {showReset ? <span> · 17:30</span> : null}
    </div>
  );
}

export type HtmlIconStyle = "pure" | "badge" | "solid";

export function SurfacePreviewFrame({
  kind,
  children,
}: {
  kind: "tray" | "float" | "taskbar";
  children: ReactNode;
}) {
  return (
    <aside className="s5-stage" aria-label="实时预览">
      <div className="s5-preview-label">实时预览</div>
      <div className={`s5-stage-frame s5-stage-frame--${kind}`}>{children}</div>
    </aside>
  );
}
