import { useEffect, useRef, useState } from "react";
import { reanchorTrayPanel, revealTrayPanelWindow } from "../lib/tauri";

export interface TrayPanelLayoutOptions {
  /** Wait until the first provider/cache snapshot is available. */
  canMeasure: boolean;
  /** The detached flyout is visible when its React surface is mounted. */
  isOpen?: boolean;
}

export interface TrayPanelLayout {
  layoutReady: boolean;
}

/**
 * Owns only the flyout handshake:
 *
 * 1. re-anchor the native window above the tray icon;
 * 3. reveal the native window after the transparent shell is ready.
 *
 * Window size is deliberately not touched here. The Rust flyout builder owns
 * the reference size, minimum bounds and remembered user resize. A frontend
 * setSize call would undo a drag as soon as React refreshes or remounts.
 */
export function useTrayPanelLayout({
  canMeasure,
  isOpen = true,
}: TrayPanelLayoutOptions): TrayPanelLayout {
  const [layoutReady, setLayoutReady] = useState(false);
  const revealedRef = useRef(false);

  useEffect(() => {
    if (!isOpen || !canMeasure || revealedRef.current) return;

    let cancelled = false;
    void (async () => {
      await Promise.resolve(reanchorTrayPanel()).catch(() => {});
      if (cancelled) return;

      revealedRef.current = true;
      setLayoutReady(true);
      await Promise.resolve(revealTrayPanelWindow()).catch(() => {});
    })();

    return () => {
      cancelled = true;
    };
  }, [canMeasure, isOpen]);

  return { layoutReady };
}
