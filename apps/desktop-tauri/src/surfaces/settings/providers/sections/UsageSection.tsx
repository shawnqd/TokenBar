import type {
  ProviderDetail,
  RateWindowSnapshot,
} from "../../../../types/bridge";
import type { LocaleKey } from "../../../../i18n/keys";
import { ProviderBalanceBlock } from "../../../../components/ProviderBalanceBlock";
import {
  isMeaningfulQuotaWindow,
  ProviderQuotaBlock,
  quotaWindowLabel,
} from "../../../../components/ProviderQuotaBlock";
import { getProviderDetailBalance } from "../../../../lib/providerBalance";
import type { QuotaDisplayContext } from "../../../../lib/quotaDisplay";

interface Props {
  provider: ProviderDetail;
  /** Follows the previewed component's context (the dashboard cards). */
  display: QuotaDisplayContext;
  t: (key: LocaleKey) => string;
}

interface BarSpec {
  key: string;
  label: string;
  rate: RateWindowSnapshot;
}

/**
 * Stacked usage bars — session / weekly / model-specific / tertiary.
 * Mirrors the bars in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel`.
 */
export function UsageSection({ provider, display, t }: Props) {
  const bars: BarSpec[] = [];
  const balanceInfo = getProviderDetailBalance(provider);

  if (
    provider.session &&
    !balanceInfo.excludeWindows.has("session") &&
    isMeaningfulQuotaWindow(provider.session)
  ) {
    bars.push({
      key: "session",
      label: quotaWindowLabel("session", provider.session, t),
      rate: provider.session,
    });
  }
  if (
    provider.weekly &&
    !balanceInfo.excludeWindows.has("weekly") &&
    isMeaningfulQuotaWindow(provider.weekly)
  ) {
    bars.push({
      key: "weekly",
      label: quotaWindowLabel("weekly", provider.weekly, t),
      rate: provider.weekly,
    });
  }
  if (provider.modelSpecific && isMeaningfulQuotaWindow(provider.modelSpecific)) {
    bars.push({
      key: "modelSpecific",
      label: t("DetailWindowModelSpecific"),
      rate: provider.modelSpecific,
    });
  }
  if (provider.tertiary && isMeaningfulQuotaWindow(provider.tertiary)) {
    bars.push({
      key: "tertiary",
      label: t("DetailWindowTertiary"),
      rate: provider.tertiary,
    });
  }
  for (const extra of provider.extraRateWindows ?? []) {
    if (!isMeaningfulQuotaWindow(extra.window)) continue;
    bars.push({
      key: extra.id,
      label: extra.title,
      rate: extra.window,
    });
  }

  if (bars.length === 0 && !balanceInfo.balance) {
    return null;
  }

  return (
    <section className="provider-detail-section">
      {/* Balance-only providers promote the balance title to the section
          header so every detail card opens with the same h4 rhythm. */}
      <h4>
        {balanceInfo.balanceOnly && balanceInfo.balance
          ? balanceInfo.balance.title
          : t("ProviderUsage")}
      </h4>
      {balanceInfo.balance && (
        <ProviderBalanceBlock
          balance={balanceInfo.balance}
          showTitle={!balanceInfo.balanceOnly}
        />
      )}
      {bars.map((b) => (
        <ProviderQuotaBlock
          key={b.key}
          title={b.label}
          rate={b.rate}
          display={display}
          usedLabel={t("PanelUsedSuffix")}
          remainingLabel={t("PanelLeftSuffix")}
          exhaustedLabel={t("DetailWindowExhausted")}
        />
      ))}
    </section>
  );
}
