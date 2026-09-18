import { Fragment, type ReactNode } from "react";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  OutputSpeedSnapshot,
  ProviderChartData,
  ProviderOutputSpeed,
  ProviderUsageSnapshot,
} from "../../types/bridge";
import type { ProviderSnapshot } from "../../core";
import type { QuotaDisplayContext } from "../../lib/quotaDisplay";
import { outputSpeedProviderId } from "../../lib/outputSpeed";
import ProviderGrid from "../../components/ProviderGrid";
import TrayCard from "./TrayCard";

export interface TrayPanelContentProps {
  /** Providers in the exact order used by the switcher grid. */
  gridProviders: ProviderUsageSnapshot[];
  /** Providers in the exact order used by the card stack. */
  visibleProviders: ProviderUsageSnapshot[];
  selectedProviderId: string | null;
  display: QuotaDisplayContext;
  densityMode: MenuBarDisplayMode;
  showProviderIcons?: boolean;
  expanded?: boolean;
  outputSpeed?: OutputSpeedSnapshot | null;
  /** Partial map keeps fixture previews honest when only one speed is available. */
  speedByProviderId?: ReadonlyMap<string, ProviderOutputSpeed>;
  outputSpeedEnabled?: boolean;
  localUsagePeriod?: LocalUsagePeriod;
  chartByProviderId?: ReadonlyMap<string, ProviderChartData>;
  /** Cards must receive this raw core snapshot whenever one exists. */
  coreById?: ReadonlyMap<string, ProviderSnapshot>;
  detail?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  onSelect: (providerId: string | null) => void;
  onReorder?: (orderedIds: string[]) => void;
  onGestureStart?: () => void;
  onGestureEnd?: () => void;
  onOpenExternalUsage?: (providerId: string) => void;
  onOpenExternalStatus?: (providerId: string) => void;
  emptyState?: ReactNode;
  footer?: ReactNode;
  resizeHandles?: ReactNode;
}

/**
 * Canonical inner tray surface. The native flyout and Settings preview own
 * different outer windows, but they must not own different grid/card markup or
 * different core-to-card data boundaries.
 */
export default function TrayPanelContent({
  gridProviders,
  visibleProviders,
  selectedProviderId,
  display,
  densityMode,
  showProviderIcons = true,
  expanded,
  outputSpeed = null,
  speedByProviderId,
  outputSpeedEnabled = true,
  localUsagePeriod,
  chartByProviderId,
  coreById,
  detail = false,
  onExpandedChange,
  onSelect,
  onReorder,
  onGestureStart,
  onGestureEnd,
  onOpenExternalUsage,
  onOpenExternalStatus,
  emptyState,
  footer,
  resizeHandles,
}: TrayPanelContentProps) {
  const resolveSpeed = (providerId: string): ProviderOutputSpeed | null => {
    const speedId = outputSpeedProviderId(providerId);
    if (!speedId) return null;
    return speedByProviderId?.get(speedId) ?? outputSpeed?.[speedId] ?? null;
  };

  return (
    <div className="tray-panel">
      <ProviderGrid
        providers={gridProviders}
        selectedProviderId={selectedProviderId}
        display={display}
        showProviderIcons={showProviderIcons}
        expanded={expanded}
        onExpandedChange={onExpandedChange}
        onSelect={onSelect}
        onReorder={onReorder}
        onGestureStart={onGestureStart}
        onGestureEnd={onGestureEnd}
      />
      <div className="flyout-body">
        {visibleProviders.length === 0
          ? emptyState
          : visibleProviders.map((provider, index) => (
              <Fragment key={provider.providerId}>
                {index > 0 && <div className="provider-stack-divider" />}
                <TrayCard
                  provider={coreById?.get(provider.providerId) ?? provider}
                  densityMode={densityMode}
                  display={display}
                  outputSpeed={resolveSpeed(provider.providerId)}
                  outputSpeedEnabled={outputSpeedEnabled}
                  localUsagePeriod={localUsagePeriod}
                  showProviderIcon={showProviderIcons}
                  detail={detail}
                  chartData={chartByProviderId?.get(provider.providerId) ?? null}
                  onOpenExternalUsage={onOpenExternalUsage}
                  onOpenExternalStatus={onOpenExternalStatus}
                />
              </Fragment>
            ))}
      </div>
      {footer}
      {resizeHandles}
    </div>
  );
}
