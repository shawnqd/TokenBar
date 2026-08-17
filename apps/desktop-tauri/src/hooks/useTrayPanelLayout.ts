import { useEffect, useRef, useState } from "react";
import { reanchorTrayPanel, revealTrayPanelWindow } from "../lib/tauri";

async function waitForFonts(): Promise<void> {
  // WebView2 implements the CSS Font Loading API. Keep the guard for tests and
  // non-browser renderers without weakening the production readiness gate.
  if (document.fonts?.ready) {
    await document.fonts.ready;
  }
}

function nextPaint(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => resolve());
  });
}

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
 * 1. re-anchor the hidden native window above the tray icon;
 * 2. wait for fonts, commit the ready class, and let WebView2 submit two frames;
 * 3. reveal only after the transparent rounded shell has reached the compositor.
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

      await waitForFonts().catch(() => {});
      if (cancelled) return;

      // React must commit the ready class while the native window is still
      // hidden. Two animation-frame boundaries are intentional: the first
      // lets React/style/layout settle, the second proves a frame containing
      // that settled transparent shell has been submitted to WebView2. This
      // is a render lifecycle gate, not a guessed time delay.
      setLayoutReady(true);
      await nextPaint();
      if (cancelled) return;
      await nextPaint();
      if (cancelled) return;

      revealedRef.current = true;
      await Promise.resolve(revealTrayPanelWindow()).catch(() => {});
    })();

    return () => {
      cancelled = true;
    };
  }, [canMeasure, isOpen]);

  return { layoutReady };
}
