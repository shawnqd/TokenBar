import { useEffect, useState } from "react";
import type { ProviderDetail } from "../../../types/bridge";
import {
  getProviderDetail,
  openProviderDashboard,
  openProviderStatusPage,
  refreshProviders,
  triggerProviderLogin,
} from "../../../lib/tauri";

function IconRefresh() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M13.2 7.1A5.2 5.2 0 0 0 3.5 5.9M3.3 3.2v3.9h3.9" />
      <path d="M2.8 8.9A5.2 5.2 0 0 0 12.5 10.1M12.7 12.8V8.9H8.8" />
    </svg>
  );
}

function IconSwitch() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <circle cx="6" cy="5" r="2.1" />
      <path d="M2.6 12.2c.4-2 1.7-3 3.4-3s3 1 3.4 3" />
      <path d="M10.2 5.5h3.4M12 3.7v3.6" />
    </svg>
  );
}

function IconDash() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
      <rect x="2.2" y="2.2" width="11.6" height="11.6" rx="2" />
      <path d="M5 10.2V7.2M8 10.2V5.5M11 10.2V8" />
    </svg>
  );
}

function IconStatus() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
      <circle cx="8" cy="8" r="5.2" />
      <path d="M8 5.2V8l2 1.4" />
    </svg>
  );
}

function IconBuy() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
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
              await refreshProviders();
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
            void run(async () => {
              await triggerProviderLogin(providerId);
              await refreshProviders();
            })
          }
        >
          <IconSwitch />
          切换账号
        </button>
      ) : null}
      {detail?.dashboardUrl ? (
        <button
          type="button"
          className="s5-pd-act"
          disabled={busy}
          onClick={() => void run(() => openProviderDashboard(providerId))}
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
          onClick={() => void run(() => openProviderStatusPage(providerId))}
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
          onClick={() => void run(() => openProviderDashboard(providerId))}
        >
          <IconBuy />
          买额度
        </button>
      ) : null}
    </div>
  );
}
