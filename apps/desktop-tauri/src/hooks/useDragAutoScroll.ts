import { useEffect, type RefObject } from "react";

/**
 * Continuous auto-scroll for an HTML5 drag over `target`, following the
 * @hello-pangea/dnd model:
 *  - the pointer is tracked with a window-level `dragover` listener, so it
 *    keeps reporting wherever the cursor is in the window. Sortable's
 *    built-in auto-scroll does the opposite: `_handleAutoScroll` clears all
 *    its scroll timers the moment the element under the cursor has no
 *    scrollable ancestor (e.g. the search box above the provider list), so a
 *    row dragged toward the top of a long list stopped scrolling as soon as
 *    the pointer crossed out of the list;
 *  - a rAF loop runs for the whole drag session instead of timers restarted
 *    per event;
 *  - the pointer being *outside* the container — above its top or below its
 *    bottom edge — scrolls at full speed instead of stopping.
 *
 * `active` follows the drag lifecycle (Sortable onStart/onEnd). A window
 * `dragend`/`drop` also drops the last pointer position, so a missed end
 * event can never leave the list scrolling on a stale cursor position.
 */
export function useDragAutoScroll(
  target: RefObject<HTMLElement | null>,
  active: boolean,
): void {
  useEffect(() => {
    if (!active) return;
    const el = target.current;
    if (!el) return;

    const EDGE = 48;
    const MAX_SPEED = 16;
    let pointerY: number | null = null;
    let rafId = 0;

    const step = () => {
      if (pointerY !== null) {
        const rect = el.getBoundingClientRect();
        let speed = 0;
        if (pointerY < rect.top + EDGE) {
          speed = -Math.min(
            MAX_SPEED,
            Math.ceil(((rect.top + EDGE - pointerY) / EDGE) * MAX_SPEED),
          );
        } else if (pointerY > rect.bottom - EDGE) {
          speed = Math.min(
            MAX_SPEED,
            Math.ceil(((pointerY - (rect.bottom - EDGE)) / EDGE) * MAX_SPEED),
          );
        }
        if (speed !== 0) el.scrollTop += speed;
      }
      rafId = requestAnimationFrame(step);
    };
    rafId = requestAnimationFrame(step);

    const trackPointer = (e: DragEvent) => {
      pointerY = e.clientY;
    };
    const forgetPointer = () => {
      pointerY = null;
    };
    window.addEventListener("dragover", trackPointer);
    window.addEventListener("dragend", forgetPointer);
    window.addEventListener("drop", forgetPointer);
    return () => {
      cancelAnimationFrame(rafId);
      window.removeEventListener("dragover", trackPointer);
      window.removeEventListener("dragend", forgetPointer);
      window.removeEventListener("drop", forgetPointer);
    };
  }, [active, target]);
}
