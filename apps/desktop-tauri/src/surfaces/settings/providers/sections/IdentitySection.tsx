import type { ReactNode } from "react";
import type { ProviderDetail } from "../../../../types/bridge";
import type { LocaleKey } from "../../../../i18n/keys";
import { getProviderDetailBalance } from "../../../../lib/providerBalance";

interface Props {
  provider: ProviderDetail;
  subtitle: string;
  actions?: ReactNode;
  t: (key: LocaleKey) => string;
}

/**
 * Compact HTML header: name + “plan · account”, actions on the right.
 * The list already shows the brand mark — do not repeat a large icon here.
 */
export function IdentitySection({ provider, subtitle, actions, t: _t }: Props) {
  const { suppressPlan } = getProviderDetailBalance(provider);
  const plan = suppressPlan ? null : displayIdentityValue(provider.plan);
  const account = provider.email ?? provider.organization;
  const who = [plan, account].filter(Boolean).join(" · ");
  const line = who || subtitle;

  return (
    <header className="provider-detail-header-block">
      <div className="provider-detail-header">
        <div className="provider-detail-title-group">
          <div className="provider-detail-title">{provider.displayName}</div>
          {line ? <div className="provider-detail-subtitle">{line}</div> : null}
        </div>
        {actions ? <div className="provider-detail-header-actions">{actions}</div> : null}
      </div>
    </header>
  );
}

function displayIdentityValue(value: string | null): string | null {
  if (!value) return null;
  if (value.trim().toLowerCase() === "default_claude_ai") return "Claude AI";
  return value;
}
