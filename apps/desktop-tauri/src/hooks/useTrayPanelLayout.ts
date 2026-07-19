import { useCallback, useEffect, useRef, useState } from "react";
import {
  getCurrentWindow,
  LogicalSize,
  PhysicalSize,
} from "@tauri-apps/api/window";
import {
  getWorkAreaRect,
  reanchorTrayPanel,
  revealTrayPanelWindow,
} from "../lib/tauri";

const TRAY_WIDTH = 328;
const TRAY_MAX_MEASURE_HEIGHT = 920;
const TRAY_OVERVIEW_MIN_HEIGHT = 200;
// Minimal mode's overview has no card stack and no footer — just the
// provider grid — so it's much shorter than a normal overview (grid +
// cards). Flooring it at TRAY_OVERVIEW_MIN_HEIGHT left a large empty gap
// below the grid; this floor only needs to cover a single icon row.
const TRAY_MINIMAL_OVERVIEW_MIN_HEIGHT = 90;
// A single detail card can be as short as an error message (no chart, no
// cost breakdown) — 420 was tuned for a fully-populated card and left a
// large empty gap below the context-action buttons for anything shorter.
// `Math.max(contentHeight, minHeight)` below still lets richer cards grow
// past this floor; it only needs to be tall enough to avoid a degenerate
// sliver for the shortest realistic content.
const TRAY_DETAIL_MIN_HEIGHT = 260;
const TRAY_DENSE_OVERVIEW_HEIGHT = 776;

export interface TrayPanelLayoutOptions {
  canMeasure: boolean;
  denseOverview: boolean;
  detailMode: boolean;
  /** Minimal display mode showing only the provider grid (no card stack, no footer). */
  minimalOverview?: boolean;
  layoutKey: string;
  /** Auto-fit the window to its content (until the user sets a size). */
  autoFit?: boolean;
  /** The user's remembered size, re-applied + re-anchored each time the flyout
   *  opens. `null` when the user has not resized yet. */
  fixedSize?: [number, number] | null;
  /** Default logical flyout height used until the user drag-resizes the
   * window; the user's saved physical height (from `fixedSize`) always wins
   * over this once set. Card content never dictates the window height. */
  fixedLogicalHeight?: number;
  /** Whether the flyout is currently open (surface mode === trayPanel). Used as
   *  the "just opened" trigger for the fixed-size restore + re-anchor. */
  isOpen?: boolean;
  /** Called with the new logical size on a genuine user drag-resize. */
  onUserResize?: (width: number, height: number) => void;
}

export interface TrayPanelLayout {
  layoutReady: boolean;
  requestLayout: () => void;
}

export function useTrayPanelLayout({
  canMeasure,
  denseOverview,
  detailMode,
  minimalOverview = false,
  layoutKey,
  autoFit = true,
  fixedSize = null,
  fixedLogicalHeight,
  isOpen = false,
  onUserResize,
}: TrayPanelLayoutOptions): TrayPanelLayout {
  const [layoutReady, setLayoutReady] = useState(false);
  const [layoutRevision, setLayoutRevision] = useState(0);
  const layoutReadyRef = useRef(false);
  const resizeRunRef = useRef(0);
  const layoutTimerRef = useRef<number | undefined>(undefined);
  // Ignore ResizeObserver feedback from our own auto-fit style mutations.
  const isMeasuringRef = useRef(false);
  // The window's actual PHYSICAL size after the last resize WE performed. The
  // onResized event also reports physical pixels, so comparing physical-to-
  // physical needs no scale factor — Tauri scaleFactor / webview devicePixelRatio
  // / Win32 can all disagree on a scaled display, and that disagreement is what
  // compounded a per-open size growth.
  const lastSizeRef = useRef<{ width: number; height: number } | null>(null);
  const programmaticInFlightRef = useRef(0);
  // Auto-fit tracks its last LOGICAL target separately (lastSizeRef is physical)
  // so its "did the content size change?" check stays in content pixels.
  const autoFitLogicalRef = useRef<{ width: number; height: number } | null>(
    null,
  );
  const fixedSizeRef = useRef(fixedSize);
  useEffect(() => {
    fixedSizeRef.current = fixedSize;
  }, [fixedSize]);
  const onUserResizeRef = useRef(onUserResize);
  useEffect(() => {
    onUserResizeRef.current = onUserResize;
  }, [onUserResize]);

  // Resize the window and record the resulting ACTUAL physical size. Wrapped in
  // an in-flight counter so the Resized event(s) this triggers can never be
  // mistaken for a user drag, regardless of event timing.
  const applySize = useCallback(
    async (size: LogicalSize | PhysicalSize): Promise<void> => {
      const win = getCurrentWindow();
      programmaticInFlightRef.current += 1;
      try {
        await win.setSize(size);
        const actual = await win.innerSize();
        lastSizeRef.current = { width: actual.width, height: actual.height };
      } catch {
        /* ignore */
      } finally {
        programmaticInFlightRef.current -= 1;
      }
    },
    [],
  );

  // Report genuine user drag-resizes. Ignore resizes that fire while WE are
  // resizing (in-flight counter) or whose physical size still matches the last
  // size we applied; anything else is the user dragging the border. Everything
  // is in PHYSICAL pixels — no scale conversion, so it can't drift.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    const win = getCurrentWindow();
    void (async () => {
      try {
        unlisten = await win.onResized(({ payload }) => {
          if (programmaticInFlightRef.current > 0) return;
          const last = lastSizeRef.current;
          if (
            last &&
            Math.abs(payload.width - last.width) <= 3 &&
            Math.abs(payload.height - last.height) <= 3
          ) {
            return;
          }
          onUserResizeRef.current?.(payload.width, payload.height);
        });
      } catch {
        unlisten = undefined;
      }
      if (cancelled) unlisten?.();
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Fixed-height flyout: on each open, apply the remembered width (or default
  // width) and one stable logical height, then re-anchor and reveal. Content
  // scrolls inside the window instead of resizing the OS surface.
  const hasFixedSize = fixedSize != null;
  useEffect(() => {
    if (autoFit || !isOpen || !canMeasure) return;
    const fixed = fixedSizeRef.current;
    let cancelled = false;
    void (async () => {
      const win = getCurrentWindow();
      const scale = await win.scaleFactor().catch(() => window.devicePixelRatio || 1);
      const width = fixed?.[0] ?? Math.round(TRAY_WIDTH * scale);
      // `fixed` holds the user's remembered PHYSICAL size — once they've
      // drag-resized, that height wins every future open. Only fall back to
      // the default logical height when nothing has been saved yet.
      const height =
        fixed?.[1] ??
        (fixedLogicalHeight !== undefined
          ? Math.round(fixedLogicalHeight * scale)
          : undefined);
      if (!height) return;
      await applySize(new PhysicalSize(width, height));
      await Promise.resolve(reanchorTrayPanel()).catch(() => {});
      if (cancelled) return;
      layoutReadyRef.current = true;
      setLayoutReady(true);
      await Promise.resolve(revealTrayPanelWindow()).catch(() => {});
    })();
    return () => {
      cancelled = true;
    };
  }, [autoFit, isOpen, canMeasure, hasFixedSize, fixedLogicalHeight, applySize]);

  const requestLayout = useCallback(() => {
    if (layoutTimerRef.current !== undefined) {
      window.clearTimeout(layoutTimerRef.current);
    }
    layoutTimerRef.current = window.setTimeout(() => {
      setLayoutRevision((current) => current + 1);
    }, layoutReadyRef.current ? 100 : 16);
  }, []);

  useEffect(() => {
    requestLayout();
  }, [layoutKey, requestLayout]);

  useEffect(() => {
    const surface = document.querySelector<HTMLElement>(".menu-surface--tray");
    if (!surface || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (!isMeasuringRef.current) requestLayout();
    });
    observer.observe(surface);
    return () => observer.disconnect();
  }, [requestLayout]);

  useEffect(() => {
    return () => {
      if (layoutTimerRef.current !== undefined) {
        window.clearTimeout(layoutTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!autoFit || !canMeasure) return;

    const minHeight = detailMode
      ? TRAY_DETAIL_MIN_HEIGHT
      : denseOverview
        ? TRAY_DENSE_OVERVIEW_HEIGHT
        : minimalOverview
          ? TRAY_MINIMAL_OVERVIEW_MIN_HEIGHT
          : TRAY_OVERVIEW_MIN_HEIGHT;

    const resize = async () => {
      const run = ++resizeRunRef.current;
      const surface = document.querySelector<HTMLElement>(".menu-surface--tray");
      if (!surface) return;
      // Whether this pass found the panel already visible — used below to
      // decide whether a genuine resize should be masked behind a fade
      // (re-measures that don't end up resizing, e.g. a background data
      // refresh with unchanged content height, must NOT dip opacity, or
      // every periodic refresh would flicker the whole panel for no reason).
      const wasReady = layoutReadyRef.current;
      isMeasuringRef.current = true;
      const html = document.documentElement;
      const pageBody = document.body;
      const workArea = await getWorkAreaRect().catch(() => null);
      const maxHeight = Math.max(
        minHeight,
        Math.min(
          TRAY_MAX_MEASURE_HEIGHT,
          (workArea?.height ?? TRAY_MAX_MEASURE_HEIGHT) - 16,
        ),
      );

      const body = surface.querySelector<HTMLElement>(".menu-surface__body");
      const stack = surface.querySelector<HTMLElement>(".menu-stack");

      const previous = {
        htmlOverflow: html.style.overflow,
        bodyOverflow: pageBody.style.overflow,
        bodyMinHeight: pageBody.style.minHeight,
        surfaceMinHeight: surface.style.minHeight,
        surfaceHeight: surface.style.height,
        surfaceMaxHeight: surface.style.maxHeight,
        surfaceOverflow: surface.style.overflow,
        bodyInnerOverflow: body?.style.overflow,
        bodyFlex: body?.style.flex,
        stackOverflow: stack?.style.overflow,
      };
      let committedHeight = false;
      let committedHeightPx: number | null = null;

      html.style.overflow = "visible";
      pageBody.style.overflow = "visible";
      pageBody.style.minHeight = "0";
      surface.style.minHeight = "0";
      surface.style.height = "auto";
      surface.style.maxHeight = "none";
      surface.style.overflow = "visible";
      if (body) {
        body.style.overflow = "visible";
        body.style.flex = "0 0 auto";
      }
      if (stack) {
        stack.style.overflow = "visible";
      }

      const revealPanel = async () => {
        if (run !== resizeRunRef.current) return;
        layoutReadyRef.current = true;
        setLayoutReady(true);
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        if (run === resizeRunRef.current) {
          await Promise.resolve(revealTrayPanelWindow()).catch(() => {});
        }
      };

      // Suppress every Resized event this whole auto-fit pass causes (the burst
      // of setSize calls + any that arrive shortly after) so none is mistaken
      // for a user drag. The trailing delay absorbs late-delivered events.
      programmaticInFlightRef.current += 1;
      try {
        if (!layoutReadyRef.current) {
          autoFitLogicalRef.current = { width: TRAY_WIDTH, height: minHeight };
          await applySize(new LogicalSize(TRAY_WIDTH, minHeight));
        }

        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

        if (run !== resizeRunRef.current) return;

        const surfaceRect = surface.getBoundingClientRect();
        let contentHeight = Math.max(
          surface.scrollHeight,
          Math.ceil(surfaceRect.height),
        );
        let maxBottom = surfaceRect.top + contentHeight;
        const bodyRect = body?.getBoundingClientRect();
        if (bodyRect && bodyRect.height > 0 && bodyRect.bottom > maxBottom) {
          maxBottom = bodyRect.bottom;
        }
        const footer = surface.querySelector<HTMLElement>(".menu-surface__footer");
        const footerRect = footer?.getBoundingClientRect();
        if (footerRect && footerRect.height > 0 && footerRect.bottom > maxBottom) {
          maxBottom = footerRect.bottom;
        }
        contentHeight = Math.ceil(maxBottom - surfaceRect.top) + 4;

        const height = Math.min(Math.max(contentHeight, minHeight), maxHeight);
        // Keep the flyout itself at the measured/capped height.  This is what
        // lets its header and footer stay put while only the card body scrolls
        // once a provider list is taller than the available work area.
        surface.style.height = `${height}px`;
        surface.style.maxHeight = `${height}px`;
        surface.style.overflow = "hidden";
        if (body) {
          body.style.overflowY = "auto";
          body.style.flex = "1 1 auto";
        }
        committedHeight = true;
        committedHeightPx = height;

        const previousSize = autoFitLogicalRef.current;
        const shouldResize =
          previousSize === null ||
          previousSize.width !== TRAY_WIDTH ||
          Math.abs(previousSize.height - height) > 2;
        if (shouldResize) {
          // A real content-height change (switching providers, expanding
          // the grid, etc.) is about to trigger a native window resize +
          // reanchor on an already-visible panel — mask that behind the
          // same brief fade used for the initial open instead of showing
          // the raw jump. Skipped on the very first pass (wasReady is
          // false then) since the panel isn't visible yet anyway.
          if (wasReady) {
            setLayoutReady(false);
          }
          autoFitLogicalRef.current = { width: TRAY_WIDTH, height };
          await applySize(new LogicalSize(TRAY_WIDTH, height));
          await Promise.resolve(reanchorTrayPanel()).catch(() => {});
        }

        await revealPanel();
      } catch (error) {
        console.warn("CodexBar tray panel resize failed", error);
        void revealPanel();
      } finally {
        if (!committedHeight) {
          surface.style.maxHeight = previous.surfaceMaxHeight;
          surface.style.minHeight = previous.surfaceMinHeight;
          surface.style.height = previous.surfaceHeight;
          surface.style.overflow = previous.surfaceOverflow;
        } else if (committedHeightPx !== null) {
          surface.style.minHeight = "0";
          surface.style.height = `${committedHeightPx}px`;
          surface.style.maxHeight = `${committedHeightPx}px`;
          surface.style.overflow = "hidden";
        }
        html.style.overflow = previous.htmlOverflow;
        pageBody.style.overflow = previous.bodyOverflow;
        pageBody.style.minHeight = previous.bodyMinHeight;
        if (body) {
          if (committedHeight) {
            body.style.overflowY = "auto";
            body.style.flex = "1 1 auto";
          } else {
            body.style.overflow = previous.bodyInnerOverflow ?? "";
            body.style.flex = previous.bodyFlex ?? "";
          }
        }
        if (stack) {
          stack.style.overflow = previous.stackOverflow ?? "";
        }
        window.setTimeout(() => {
          programmaticInFlightRef.current = Math.max(
            0,
            programmaticInFlightRef.current - 1,
          );
          isMeasuringRef.current = false;
        }, 200);
      }
    };

    const timer = window.setTimeout(
      () => void resize(),
      layoutReadyRef.current ? 25 : 0,
    );

    return () => {
      window.clearTimeout(timer);
      resizeRunRef.current += 1;
    };
  }, [
    autoFit,
    canMeasure,
    denseOverview,
    detailMode,
    minimalOverview,
    layoutRevision,
    applySize,
  ]);

  return { layoutReady, requestLayout };
}
