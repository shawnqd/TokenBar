import { useEffect, useState } from "react";
import type { ProviderDetail } from "../../../types/bridge";
import { getProviderDetail, onProviderUpdated } from "../../../lib/tauri";
import { useDispatchAction } from "../../../core/useCoreBridge";

const iconProps = {
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: "1.2",
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

function IconRefresh() {
  return (
    <svg {...iconProps} viewBox="0 0 24 24" strokeWidth="1.8">
      <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
      <path d="M3 21v-5h5" />
    </svg>
  );
}

function IconDash() {
  return (
    <svg {...iconProps}>
      <rect x="2.2" y="2.2" width="11.6" height="11.6" rx="2" />
      <path d="M5 10.2V7.2M8 10.2V5.5M11 10.2V8" />
    </svg>
  );
}

function IconStatus() {
  return (
    <svg {...iconProps}>
      <circle cx="8" cy="8" r="5.2" />
      <path d="M8 5.2V8l2 1.4" />
    </svg>
  );
}

function IconBuy() {
  return (
    <svg {...iconProps}>
      <circle cx="8" cy="8" r="5.2" />
      <path d="M8 5.4v5.2M5.4 8h5.2" />
    </svg>
  );
}

export default function ProviderActionBar({
  providerId,
  motion,
  onError,
  onDetail,
}: {
  providerId: string;
  motion: boolean;
  onError: (message: string | null) => void;
  onDetail?: (detail: ProviderDetail | null) => void;
}) {
  const [detail, setDetail] = useState<ProviderDetail | null>(null);
  const [busy, setBusy] = useState(false);
  const dispatch = useDispatchAction();

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    getProviderDetail(providerId)
      .then((next) => {
        if (cancelled) return;
        setDetail(next);
        onDetail?.(next);
      })
      .catch((error) => {
        if (cancelled) return;
        onError(String(error));
      });
    return () => {
      cancelled = true;
    };
  }, [providerId, onDetail, onError]);

  // Upstream parity: the detail pane re-reads the authoritative DTO when the
  // backend emits `provider-updated`, so list rows (projection) and the
  // detail card (get_provider_detail) can never drift after a background or
  // tray-side refresh. A payload for another provider is ignored.
  useEffect(() => {
    let disposed = false;
    const unlisten = onProviderUpdated((snapshot) => {
      if (disposed) return;
      if (snapshot.providerId && snapshot.providerId !== providerId) return;
      void getProviderDetail(providerId)
        .then((next) => {
          if (disposed) return;
          setDetail(next);
          onDetail?.(next);
        })
        .catch(() => {
          // Keep the last known detail; the next event retries.
        });
    });
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [providerId, onDetail]);

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    onError(null);
    try {
      await work();
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="s5-pd-links">
      <button
        type="button"
        className={`s5-pd-act${busy && motion ? " is-busy" : ""}`}
        disabled={busy}
        onClick={() =>
            void run(async () => {
              await dispatch({
                type: "refresh",
                target: { kind: "provider", providerId },
                force: true,
              });
              const next = await getProviderDetail(providerId);
              setDetail(next);
              onDetail?.(next);
            })
          }
      >
        <IconRefresh />
        刷新
      </button>
      {detail?.dashboardUrl ? (
        <button
          type="button"
          className="s5-pd-act"
          disabled={busy}
          onClick={() =>
            void run(() =>
              dispatch({
                type: "openExternalUsage",
                target: { kind: "provider", providerId },
              }).then(() => undefined),
            )
          }
        >
          <IconDash />
          用量页
        </button>
      ) : null}
      {detail?.statusPageUrl ? (
        <button
          type="button"
          className="s5-pd-act"
          disabled={busy}
          onClick={() =>
            void run(() =>
              dispatch({
                type: "openExternalStatus",
                target: { kind: "provider", providerId },
              }).then(() => undefined),
            )
          }
        >
          <IconStatus />
          状态
        </button>
      ) : null}
      {detail?.buyCreditsUrl ? (
        <button
          type="button"
          className="s5-pd-act"
          disabled={busy}
          onClick={() => {
            const url = detail.buyCreditsUrl;
            if (!url) return;
            void run(() =>
              dispatch({
                type: "openExternalUrl",
                target: { kind: "app" },
                url,
              }).then(() => undefined),
            );
          }}
        >
          <IconBuy />
          买额度
        </button>
      ) : null}
    </div>
  );
}
