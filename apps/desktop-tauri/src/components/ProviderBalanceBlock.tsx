import type { BalanceView } from "../lib/providerBalance";

export function ProviderBalanceBlock({
  balance,
  className = "",
  showTitle = true,
}: {
  balance: BalanceView;
  className?: string;
  /** Hide the internal title when the host already renders it as a header. */
  showTitle?: boolean;
}) {
  const classes = ["provider-balance", className].filter(Boolean).join(" ");

  return (
    <section className={classes} data-balance-kind={balance.kind}>
      {showTitle && (
        <div className="provider-balance__title">{balance.title}</div>
      )}
      {balance.kind === "balance" ? (
        <div className="provider-balance__row">
          <span
            className="provider-balance__amount"
            data-unavailable={balance.unavailable ? "true" : undefined}
          >
            {balance.amount}
          </span>
          {balance.breakdown && (
            <span className="provider-balance__note">{balance.breakdown}</span>
          )}
        </div>
      ) : (
        <div className="provider-balance__status">{balance.amount}</div>
      )}
    </section>
  );
}
