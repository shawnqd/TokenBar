import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { SettingsSnapshot, SettingsUpdate } from "../types/bridge";
import { getSettingsSnapshot, invokeSurfaceAction } from "../lib/tauri";

interface UseSettingsReturn {
  settings: SettingsSnapshot;
  saving: boolean;
  error: string | null;
  update: (patch: SettingsUpdate) => Promise<void>;
}

/** Sentinel the save watchdog stores in `error` when the backend command
 *  never settles. Settings.tsx translates it (it owns the locale); anything
 *  else in `error` is a raw backend message shown verbatim. */
export const SAVE_TIMEOUT_ERROR = "__save_timeout__";

/** Watchdog budget for one save. Mutable holder so tests can shorten it —
 *  fake timers here fight the test library's own async plumbing. */
export const SAVE_WATCHDOG_MS = { value: 20_000 };

/**
 * Manages the current settings state and exposes a mutation helper that
 * persists changes through the Tauri bridge and refreshes the local copy.
 */
export function useSettings(initial: SettingsSnapshot): UseSettingsReturn {
  const [settings, setSettings] = useState<SettingsSnapshot>(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const latestSeq = useRef(0);
  const syncSeq = useRef(0);
  // Separate the UI save lifecycle from the response sequence. A save can be
  // superseded by another local edit or by a timeout, and only its owner may
  // release the shared `saving` indicator.
  const activeSaveSeq = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;

    setSettings(initial);

    getSettingsSnapshot()
      .then((fresh) => {
        if (!cancelled) {
          setSettings(fresh);
        }
      })
      .catch(() => {
        // Keep the bootstrap snapshot if the background sync fails.
      });

    return () => {
      cancelled = true;
    };
  }, [initial]);

  // Live-sync when settings change in ANOTHER window. The detached Settings
  // window and the main/PopOut window are separate webviews with separate
  // React state, so the in-window CustomEvent below never reaches them. Rust
  // broadcasts "settings-changed" after every persisted update; re-fetch the
  // snapshot so this surface (e.g. the PopOut window scale) re-renders live.
  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    // `Promise.resolve` tolerates test mocks that return a bare unlisten fn (or
    // undefined) instead of a promise; the `active` flag handles unmounting
    // before the listener finishes registering.
    Promise.resolve(
      listen("settings-changed", () => {
        // Cross-window refreshes have their own sequence. The Rust event is
        // broadcast to every webview, including the one that issued the save;
        // it must not invalidate that save's local completion path.
        const seq = ++syncSeq.current;
        getSettingsSnapshot()
          .then((fresh) => {
            if (seq === syncSeq.current) {
              setSettings(fresh);
            }
          })
          .catch(() => {
            // Keep the current copy if the refresh fails.
          });
      }),
    )
      .then((fn) => {
        if (active) {
          unlisten = fn;
        } else {
          fn?.();
        }
      })
      .catch(() => {});
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const update = useCallback(async (patch: SettingsUpdate) => {
    // Apply optimistically so toggles/selects feel instant — the local
    // round trip to the Rust bridge is fast enough that waiting for it
    // before reflecting the change reads as a page-wide flash (every
    // `disabled={saving}` control dims and undims within one frame).
    const seq = ++latestSeq.current;
    // Invalidate a snapshot fetch started by an older cross-window event so it
    // cannot overwrite this optimistic local edit.
    syncSeq.current += 1;
    setSettings((prev) => ({ ...prev, ...patch }));
    setError(null);

    // Only surface the "saving" state (which disables controls) if the
    // round trip is slow enough to notice — avoids a flash-disable on
    // every click for the common fast-save case.
    activeSaveSeq.current = seq;
    const savingIndicatorTimer = window.setTimeout(() => {
      if (activeSaveSeq.current === seq) {
        setSaving(true);
      }
    }, 200);

    // Watchdog: `update_settings` does disk writes, AppState locking and tray
    // refreshes on the command thread; contending with a background refresh
    // has left it pending for minutes, which pinned the "保存中…" pill and the
    // disabled controls forever. 20s is far longer than any legitimate save,
    // so hitting it means the command is wedged: release the UI, surface a
    // timeout, and bump the sequence so the hung command's eventual settle is
    // discarded instead of flashing a saved toast over it.
    const saveWatchdog = window.setTimeout(() => {
      if (activeSaveSeq.current !== seq) return;
      activeSaveSeq.current = null;
      latestSeq.current += 1;
      syncSeq.current += 1;
      // The indicator timer belongs to this save; the normal `finally` block
      // cannot clear it while a wedged command is still pending.
      window.clearTimeout(savingIndicatorTimer);
      setError(SAVE_TIMEOUT_ERROR);
      setSaving(false);
    }, SAVE_WATCHDOG_MS.value);

    try {
      await invokeSurfaceAction({
        type: "updateSettings",
        target: { kind: "settings" },
        patch,
      });
      // Discard stale responses from earlier requests that arrived out of
      // order — the optimistic value is already the latest.
      if (seq !== latestSeq.current) return;
      const next = await getSettingsSnapshot();
      // A settings-changed fetch may have started while this save was in
      // flight. The local persisted snapshot is newer and wins this race.
      if (seq !== latestSeq.current) return;
      syncSeq.current += 1;
      setSettings(next);
      if (typeof window !== "undefined") {
        window.dispatchEvent(
          new CustomEvent<SettingsSnapshot>("codexbar:settings-updated", {
            detail: next,
          }),
        );
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      // A stale request must not replace a newer request's status or restore
      // an older snapshot over the latest optimistic value.
      if (seq === latestSeq.current) {
        setError(msg);
        // Re-fetch to stay in sync with disk state on failure.
        try {
          const fresh = await getSettingsSnapshot();
          if (seq === latestSeq.current) {
            syncSeq.current += 1;
            setSettings(fresh);
          }
        } catch {
          // ignore secondary failure
        }
      }
    } finally {
      window.clearTimeout(savingIndicatorTimer);
      window.clearTimeout(saveWatchdog);
      if (activeSaveSeq.current === seq) {
        activeSaveSeq.current = null;
        setSaving(false);
      }
    }
  }, []);

  return { settings, saving, error, update };
}
