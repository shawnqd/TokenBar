import { useMemo, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  ProviderChartData,
  ProviderOutputSpeed,
  TrayIconMode,
} from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import TrayCard from "../../../surfaces/tray/TrayCard";
import ProviderGrid from "../../../components/ProviderGrid";
import { SurfacePreviewFrame } from "./HtmlSurfacePreviews";
import { catalogPreviewSnapshots } from "../previews/catalogFixtures";
import {
  quotaDisplayContext,
  quotaDisplayPreference,
  resetDisplayPreference,
} from "../../../lib/quotaDisplay";
import { V5Field, V5Num, V5Section, V5Seg, V5Toggle } from "./v5Controls";
import { useTrayCoreRecords, trayCoreStore } from "../../../surfaces/tray/trayCoreStore";
import { useOutputSpeedSnapshot } from "../../../hooks/useOutputSpeedSnapshot";
import { outputSpeedProviderId } from "../../../lib/outputSpeed";
import { coreSnapshotToBridge } from "../../../lib/trayProviders";
import { orderProviderSnapshots } from "../../../lib/providerOrder";

const TRAY_DEFAULTS = {
  menuBarDisplayMode: "detailed" as MenuBarDisplayMode,
  localUsagePeriod: "today" as LocalUsagePeriod,
  dashboardQuotaDisplay: "follow" as const,
  dashboardResetDisplay: "follow" as const,
  showAllTokenAccountsInMenu: false,
  trayIconMode: "single" as TrayIconMode,
  switcherShowsIcons: true,
  menuBarShowsHighestUsage: false,
  outputSpeedEnabled: true,
  keepTrayPanelOnSettings: true,
  trayScalePercent: 100,
};

/**
 * The preview stack for section 7.3 of TRAY_PANEL_SPECIFICATION.md: real cards
 * driven from catalog-shaped snapshots covering one row per archetype — Claude
 * (cycle quota), DeepSeek (balance), Cursor (hybrid). All three react live to
 * the density / used-remaining / reset / period / icon toggles on the left.
 */
const PREVIEW_PROVIDERS = ["claude", "deepseek", "cursor"];

const PREVIEW_CHART_DATA: Record<string, ProviderChartData> = {
  claude: {
    providerId: "claude",
    costHistory: [],
    creditsHistory: [],
    usageBreakdown: [],
    localUsage: {
      todayCost: 0.65,
      todayTokens: 42_000,
      sevenDayCost: 2.85,
      sevenDayTokens: 186_000,
      thirtyDayCost: 8.2,
      thirtyDayTokens: 520_000,
      todayTopModel: "claude-3-7-sonnet",
      sevenDayTopModel: "claude-3-7-sonnet",
      thirtyDayTopModel: "claude-3-7-sonnet",
      estimateNote: "",
    },
  },
};

const PREVIEW_CLAUDE_SPEED: ProviderOutputSpeed = {
  providerId: "claude",
  status: "recent",
  tokensPerSecond: 48.2,
  outputTokens: 1240,
  updatedAtMs: Date.now() - 30_000,
  approximate: false,
  recentSamples: [],
};

export default function TrayPanelPage({
  settings,
  set,
  saving,
  catalog,
  head,
}: SettingsPageProps) {
  const { t } = useLocale();
  const scale = settings.trayScalePercent ?? 100;
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    null,
  );
  const records = useTrayCoreRecords();
  const outputSpeedEnabled = settings.outputSpeedEnabled !== false;
  const liveSpeed = useOutputSpeedSnapshot(outputSpeedEnabled);

  const shownProviderIds = settings.enabledProviders ?? [];
  const enabledSnapshots = useMemo(() => {
    return records
      .filter(
        (record) =>
          record.snapshot != null &&
          shownProviderIds.includes(record.snapshot.providerId),
      )
      .map((record) => coreSnapshotToBridge(record.snapshot!));
  }, [records, shownProviderIds]);

  const preview = useMemo(() => {
    const rawRows =
      enabledSnapshots.length > 0
        ? orderProviderSnapshots(
            enabledSnapshots,
            catalog ?? [],
            shownProviderIds,
            settings.providerOrder,
          )
        : catalogPreviewSnapshots().filter((row) =>
            PREVIEW_PROVIDERS.includes(row.providerId),
          );

    if (!settings.menuBarShowsHighestUsage) return rawRows;
    return [...rawRows].sort(
      (left, right) => right.primary.usedPercent - left.primary.usedPercent,
    );
  }, [
    enabledSnapshots,
    catalog,
    shownProviderIds,
    settings.providerOrder,
    settings.menuBarShowsHighestUsage,
  ]);
  const display = quotaDisplayContext(settings, "dashboard");
  const quotaMode = quotaDisplayPreference(settings, "dashboard");
  const resetMode = resetDisplayPreference(settings, "dashboard");
  const effectiveQuotaLabel = display.showAsUsed
    ? t("QuotaShowUsedOption")
    : t("QuotaShowRemainingOption");
  const quotaHelp = t(
    quotaMode === "follow" ? "QuotaFollowHelper" : "QuotaOverrideHelper",
  ).replace(
    "{}",
    effectiveQuotaLabel,
  );
  const effectiveResetLabel = display.resetTimeRelative
    ? t("ResetTimeCountdownOption")
    : t("ResetTimeAbsoluteOption");
  const resetHelp = t(
    resetMode === "follow" ? "QuotaFollowHelper" : "QuotaOverrideHelper",
  ).replace(
    "{}",
    effectiveResetLabel,
  );
  const isDetail = selectedProviderId !== null;
  const density = isDetail ? "detailed" : settings.menuBarDisplayMode;
  const visible = isDetail
    ? preview.filter((row) => row.providerId === selectedProviderId)
    : preview;
  const scaleRatio = Math.max(1, Math.min(2, scale / 100));
  const previewScale = Math.min(1.04, Math.round(0.85 * scaleRatio * 100) / 100);

  return (
    <div className="s5-surf-split">
      <div className="s5-surf-fields">
        {head}
        <V5Section
          title="显示内容"
          resetLabel={t("ComponentResetDefaults")}
          resetDisabled={saving}
          onReset={() => set(TRAY_DEFAULTS)}
        >
          <V5Field label="显示密度" help="只影响总览。详情始终用详细">
            <V5Seg
              value={settings.menuBarDisplayMode}
              disabled={saving}
              options={[
                { value: "minimal", label: "简洁" },
                { value: "compact", label: "平衡" },
                { value: "detailed", label: "详细" },
              ]}
              onChange={(value) =>
                set({ menuBarDisplayMode: value as MenuBarDisplayMode })
              }
            />
          </V5Field>
          <V5Field
            label="额度数字"
            help={quotaHelp}
          >
            <V5Seg
              value={quotaMode}
              disabled={saving}
              options={[
                { value: "follow", label: t("QuotaFollowOption") },
                { value: "used", label: t("QuotaShowUsedOption") },
                { value: "remaining", label: t("QuotaShowRemainingOption") },
              ]}
              onChange={(value) =>
                set({ dashboardQuotaDisplay: value as "follow" | "used" | "remaining" })
              }
            />
          </V5Field>
          <V5Field
            label="重置时间"
            help={resetHelp}
          >
            <V5Seg
              value={resetMode}
              disabled={saving}
              options={[
                { value: "follow", label: t("QuotaFollowOption") },
                { value: "countdown", label: t("ResetTimeCountdownOption") },
                { value: "absolute", label: t("ResetTimeAbsoluteOption") },
              ]}
              onChange={(value) =>
                set({ dashboardResetDisplay: value as "follow" | "countdown" | "absolute" })
              }
            />
          </V5Field>
          <V5Field label="本地用量周期">
            <V5Seg
              value={settings.localUsagePeriod ?? "today"}
              disabled={saving}
              options={[
                { value: "today", label: "今日" },
                { value: "7d", label: "近 7 天" },
                { value: "30d", label: "近 30 天" },
              ]}
              onChange={(value) =>
                set({ localUsagePeriod: value as LocalUsagePeriod })
              }
            />
          </V5Field>
          <V5Field label="显示输出速度" help="关掉后速度格子留空">
            <V5Toggle
              on={settings.outputSpeedEnabled ?? true}
              disabled={saving}
              onChange={(v) => set({ outputSpeedEnabled: v })}
              label="显示输出速度"
            />
          </V5Field>
        </V5Section>

        <V5Section title="打开与账号">
          <V5Field
            label="设置打开时保留托盘"
            help="开着时，设置页在前也不会被点外侧关掉托盘。关掉则设置开着时点托盘外仍会关闭"
          >
            <V5Toggle
              on={settings.keepTrayPanelOnSettings ?? true}
              disabled={saving}
              onChange={(v) => set({ keepTrayPanelOnSettings: v })}
              label="设置打开时保留托盘"
            />
          </V5Field>
          <V5Field label="列出全部令牌账户" help="在托盘面板中展开已保存的全部账号">
            <V5Toggle
              on={settings.showAllTokenAccountsInMenu}
              disabled={saving}
              onChange={(v) => set({ showAllTokenAccountsInMenu: v })}
              label="列出全部令牌账户"
            />
          </V5Field>
        </V5Section>

        <V5Section title="通知区与布局">
          <V5Field
            label="托盘图标"
            help="通知区那个小图标，不是卡片上的品牌标"
          >
            <V5Seg
              value={settings.trayIconMode}
              disabled={saving}
              options={[
                { value: "single", label: "单一图标" },
                { value: "perProvider", label: "按服务商" },
              ]}
              onChange={(value) => set({ trayIconMode: value as TrayIconMode })}
            />
          </V5Field>
          <V5Field label="显示服务商图标">
            <V5Toggle
              on={settings.switcherShowsIcons}
              disabled={saving}
              onChange={(v) => set({ switcherShowsIcons: v })}
              label="显示服务商图标"
            />
          </V5Field>
          <V5Field label="总览排序">
            <V5Seg
              value={settings.menuBarShowsHighestUsage ? "usage" : "order"}
              disabled={saving}
              options={[
                { value: "usage", label: "最高用量优先" },
                { value: "order", label: "按服务商顺序" },
              ]}
              onChange={(value) =>
                set({ menuBarShowsHighestUsage: value === "usage" })
              }
            />
          </V5Field>
          <V5Field label="面板缩放" help="只放大托盘内容">
            <V5Num
              value={scale}
              min={100}
              max={200}
              unit="%"
              disabled={saving}
              onChange={(v) => set({ trayScalePercent: v })}
            />
          </V5Field>
        </V5Section>
      </div>

      <SurfacePreviewFrame kind="tray">
        <div
          className="s5-tray-flyout"
          style={{ zoom: previewScale }}
        >
          <div className="tray-panel">
            <ProviderGrid
              providers={preview}
              selectedProviderId={selectedProviderId}
              display={display}
              showProviderIcons={settings.switcherShowsIcons}
              onSelect={setSelectedProviderId}
            />
            <div className="flyout-body">
              {visible.map((snapshot, idx) => {
                const recordForSnapshot = records.find(
                  (r) => r.snapshot?.providerId === snapshot.providerId,
                );
                const liveChart = recordForSnapshot
                  ? trayCoreStore.getChartData(recordForSnapshot.key)
                  : null;
                const chartDataForCard =
                  liveChart ?? PREVIEW_CHART_DATA[snapshot.providerId] ?? null;

                const speedId = outputSpeedProviderId(snapshot.providerId);
                const speedForCard = !outputSpeedEnabled
                  ? null
                  : (speedId && liveSpeed?.[speedId]) ||
                    (snapshot.providerId === "claude" ? PREVIEW_CLAUDE_SPEED : null);

                return (
                  <div key={snapshot.providerId}>
                    {idx > 0 && <div className="provider-stack-divider" />}
                    <TrayCard
                      provider={snapshot}
                      densityMode={density}
                      display={display}
                      showProviderIcon={settings.switcherShowsIcons}
                      localUsagePeriod={settings.localUsagePeriod ?? "today"}
                      outputSpeed={speedForCard}
                      outputSpeedEnabled={outputSpeedEnabled}
                      chartData={chartDataForCard}
                      detail={isDetail}
                    />
                  </div>
                );
              })}
            </div>

          </div>
        </div>
      </SurfacePreviewFrame>
    </div>
  );
}
