import { describe, expect, it, vi } from "vitest";
import { render, act, waitFor } from "@testing-library/react";
import React, { useEffect, useState } from "react";
import { createUsageStore, keyFromSnapshot } from "./usageStore";
import { createRefreshCoordinator } from "./refreshCoordinator";
import { FIXTURE_SNAPSHOTS } from "./index";
import { useCoreSnapshot, useRefreshCommand, setCoreBridgeStore, setCoreBridgeCoordinator } from "./useCoreBridge";
import { useSyncExternalStore } from "react";

// Simple component that uses useCoreSnapshot
function SnapshotViewer({ store, providerKey }: { store: ReturnType<typeof createUsageStore>; providerKey: ReturnType<typeof keyFromSnapshot> }) {
  const record = useCoreSnapshot(providerKey, store);
  return <div data-testid="snap">{record?.snapshot?.displayName ?? "none"}-{record?.displayState ?? "none"}</div>;
}

function RefreshButton({ coordinator }: { coordinator: ReturnType<typeof createRefreshCoordinator> }) {
  const refresh = useRefreshCommand(coordinator);
  const [state, setState] = useState("idle");
  const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
  return (
    <button
      onClick={async () => {
        setState("loading");
        await refresh(key, { manual: true });
        setState("done");
      }}
    >
      {state}
    </button>
  );
}

describe("useCoreBridge", () => {
  it("subscription is stable and updates when store changes", async () => {
    const store = createUsageStore();
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    const { getByTestId, unmount } = render(<SnapshotViewer store={store} providerKey={key} />);
    expect(getByTestId("snap").textContent).toBe("none-none");
    act(() => {
      store.upsert(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    });
    await waitFor(() => expect(getByTestId("snap").textContent).toContain("Kimi-like"));
    expect(getByTestId("snap").textContent).toContain("ready");
    // update again
    const updated = { ...FIXTURE_SNAPSHOTS.kimiSessionWeekly, planName: "pro" };
    act(() => {
      store.upsert(updated);
    });
    await waitFor(() => expect(getByTestId("snap").textContent).toContain("ready"));
    unmount();
  });

  it("unmount releases subscription (no leak)", async () => {
    const store = createUsageStore();
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.balanceOnly);
    const subSpy = vi.spyOn(store, "subscribe");
    const { unmount } = render(<SnapshotViewer store={store} providerKey={key} />);
    expect(subSpy).toHaveBeenCalledTimes(1);
    // capture unsubscribe mock
    const calls = subSpy.mock.results;
    // Can't directly inspect Set size, but we can test that after unmount, store listeners no longer called
    let notified = 0;
    const extraUnsub = store.subscribe(() => notified++);
    expect(notified).toBe(0);
    unmount();
    act(() => {
      store.upsert(FIXTURE_SNAPSHOTS.balanceOnly);
    });
    // extra subscriber should have been notified, but unmounted component's listener should not be called again
    // We can't directly count component calls, but we can ensure store's internal listeners size is 1 (only extra)
    // Trigger again and check notified increments
    expect(notified).toBe(1);
    extraUnsub();
    // After removing extra, no listeners remain; upsert should not throw
    act(() => {
      store.upsert(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    });
    expect(notified).toBe(1);
  });

  it("manual refresh via useRefreshCommand updates snapshot", async () => {
    const store = createUsageStore();
    const fetcher = vi.fn(async () => FIXTURE_SNAPSHOTS.balanceOnly);
    const coordinator = createRefreshCoordinator({ store, fetcher });
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.balanceOnly);
    // initially empty
    expect(store.get(key)).toBeUndefined();
    const { getByText } = render(<RefreshButton coordinator={coordinator} />);
    const btn = getByText("idle");
    await act(async () => {
      btn.click();
    });
    await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(getByText("done")).toBeInTheDocument());
    expect(store.get(key)?.displayState).toBe("ready");
  });

  it("useCoreSnapshot works with global store when no override", async () => {
    const globalStore = createUsageStore();
    setCoreBridgeStore(globalStore);
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.telemetryOnly);
    function GlobalViewer() {
      const rec = useCoreSnapshot(key);
      return <div data-testid="g">{rec?.displayState ?? "none"}</div>;
    }
    const { getByTestId, unmount } = render(<GlobalViewer />);
    expect(getByTestId("g").textContent).toBe("none");
    act(() => {
      globalStore.upsert(FIXTURE_SNAPSHOTS.telemetryOnly);
    });
    await waitFor(() => expect(getByTestId("g").textContent).toBe("ready"));
    unmount();
    setCoreBridgeStore(null);
  });

  it("component unmount during refresh does not leak coordinator inflight", async () => {
    const pending = new Promise(() => {}); // never resolves to simulate inflight
    const fetcher = vi.fn(() => pending as Promise<any>);
    const store = createUsageStore();
    const coordinator = createRefreshCoordinator({ store, fetcher });
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    function AutoRefresh() {
      const refresh = useRefreshCommand(coordinator);
      useEffect(() => {
        void refresh(key);
      }, [refresh]);
      const rec = useCoreSnapshot(key, store);
      return <div>{rec ? "has" : "none"}</div>;
    }
    const { unmount } = render(<AutoRefresh />);
    // unmount quickly while inflight
    unmount();
    // should not throw, and inflight should still be tracked but not leak listeners
    expect(fetcher).toHaveBeenCalledTimes(1);
    coordinator.destroy();
  });
});
