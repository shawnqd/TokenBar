import { useEffect, useRef, useState } from "react";
import { getCurrentWindow, PhysicalSize } from "@tauri-apps/api/window";
import { reanchorTrayPanel, revealTrayPanelWindow } from "../lib/tauri";

const DEFAULT_TRAY_WIDTH = 328;
const DEFAULT_TRAY_HEIGHT = 776;

export interface TrayPanelLayoutOptions {
  /** Wait until the first provider/cache snapshot is available. */
  canMeasure: boolean;
  /** The detached flyout is visible when its React surface is mounted. */
  isOpen?: boolean;
  /** Fixed logical dimensions for the Windows flyout. */
  fixedLogicalWidth?: number;
  fixedLogicalHeight?: number;
}

export interface TrayPanelLayout {
  layoutReady: boolean;
}

/**
 * Owns only the fixed flyout handshake:
 *
 * 1. apply the stable physical size once WebView content is ready;
 * 2. re-anchor the window above the tray icon;
 * 3. reveal the native window after the transparent shell is ready.
 *
 * The old implementation also measured content, watched ResizeObserver,
 * persisted user sizes, and drove native resize gestures. Those paths were
 * removed with the fixed-size flyout so provider updates can never resize or
 * reposition the panel.
 */
export function useTrayPanelLayout({
  canMeasure,
  isOpen = true,
  fixedLogicalWidth = DEFAULT_TRAY_WIDTH,
  fixedLogicalHeight = DEFAULT_TRAY_HEIGHT,
}: TrayPanelLayoutOptions): TrayPanelLayout {
  const [layoutReady, setLayoutReady] = useState(false);
  const revealedRef = useRef(false);

  useEffect(() => {
    if (!isOpen || !canMeasure || revealedRef.current) return;

    let cancelled = false;
    void (async () => {
      const currentWindow = getCurrentWindow();
      const scale = await currentWindow
        .scaleFactor()
        .catch(() => globalThis.devicePixelRatio || 1);
      const size = new PhysicalSize(
        Math.round(fixedLogicalWidth * scale),
        Math.round(fixedLogicalHeight * scale),
      );

      await currentWindow.setSize(size).catch(() => {});
      await Promise.resolve(reanchorTrayPanel()).catch(() => {});
      if (cancelled) return;

      revealedRef.current = true;
      setLayoutReady(true);
      await Promise.resolve(revealTrayPanelWindow()).catch(() => {});
    })();

    return () => {
      cancelled = true;
    };
  }, [canMeasure, fixedLogicalHeight, fixedLogicalWidth, isOpen]);

  return { layoutReady };
}
