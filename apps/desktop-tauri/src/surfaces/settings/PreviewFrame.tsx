import type { ReactNode } from "react";
import "./settingsSurfaces.css";

export type PreviewFrameKind = "tray" | "float" | "taskbar";

const FRAME_SIZE: Record<PreviewFrameKind, { width: number; height: number }> = {
  tray: { width: 328, height: 256 },
  float: { width: 280, height: 156 },
  taskbar: { width: 248, height: 52 },
};

interface Props {
  kind: PreviewFrameKind;
  label: string;
  children: ReactNode;
  dimmed?: boolean;
}

/**
 * Sticky product preview. The stage box is reserved at the largest state so
 * density / orientation / width changes do not jump the controls column.
 */
export default function PreviewFrame({ kind, label, children, dimmed }: Props) {
  const size = FRAME_SIZE[kind];
  return (
    <aside className="settings-preview-frame" aria-label={label}>
      <div className="settings-preview-frame__label">{label}</div>
      <div
        className={`settings-preview-frame__stage settings-preview-frame__stage--${kind}${dimmed ? " is-off" : ""}`}
        style={{ width: size.width, height: size.height }}
      >
        {children}
      </div>
    </aside>
  );
}
