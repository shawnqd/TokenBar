import { useMemo, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  TrayIconMode,
} from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import TrayCard from "../../../surfaces/tray/TrayCard";
import ProviderGrid from "../../../components/ProviderGrid";
import { SurfacePreviewFrame } from "./HtmlSurfacePreviews";
import { catalogPreviewSnapshots } from "../previews/catalogFixtures";
import { quotaDisplayContext } from "../../../lib/quotaDisplay";
import { V5Field, V5Num, V5Section, V5Seg, V5Toggle } from "./v5Controls";

const footerIconProps = {
  width: 14,
  height: 14,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.85,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

const TRAY_DEFAULTS = {
  menuBarDisplayMode: "detailed" as MenuBarDisplayMode,
  localUsagePeriod: "today" as LocalUsagePeriod,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  showAllTokenAccountsInMenu: false,
  trayIconMode: "single" as TrayIconMode,
  switcherShowsIcons: true,
  menuBarShowsHighestUsage: false,
  outputSpeedEnabled: true,
  trayScalePercent: 100,
};

/**
 * The preview stack for section 7.3 of TRAY_PANEL_SPECIFICATION.md: real cards
 * driven from catalog-shaped snapshots covering one row per archetype — Claude
 * (cycle quota), DeepSeek (balance), Cursor (hybrid). All three react live to
 * the density / used-remaining / reset / period / icon toggles on the left.
 */
const PREVIEW_PROVIDERS = ["claude", "deepseek", "cursor"];

export default function TrayPanelPage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
  const { t } = useLocale();
  const scale = settings.trayScalePercent ?? 100;
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    null,
  );

  const preview = useMemo(() => {
    const rows = catalogPreviewSnapshots().filter((row) =>
      PREVIEW_PROVIDERS.includes(row.providerId),
    );
    if (!settings.menuBarShowsHighestUsage) return rows;
    return [...rows].sort(
      (left, right) => right.primary.usedPercent - left.primary.usedPercent,
    );
  }, [settings.menuBarShowsHighestUsage]);
  const display = quotaDisplayContext(settings, "dashboard");
  const isDetail = selectedProviderId !== null;
  const density = isDetail ? "detailed" : settings.menuBarDisplayMode;
  const visible = isDetail
    ? preview.filter((row) => row.providerId === selectedProviderId)
    : preview;
  const scaleRatio = Math.max(1, Math.min(2, scale / 100));

  return (
    <div className="s5-surf-split">
      <div className="s5-surf-fields">
        <V5Section
          title="卡片内容"
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
          <V5Field label="额度数字">
            <V5Seg
              value={settings.dashboardShowAsUsed ? "used" : "remain"}
              disabled={saving}
              options={[
                { value: "used", label: "已用" },
                { value: "remain", label: "剩余" },
              ]}
              onChange={(value) =>
                set({ dashboardShowAsUsed: value === "used" })
              }
            />
          </V5Field>
          <V5Field label="重置时间">
            <V5Seg
              value={settings.dashboardResetTimeRelative ? "rel" : "abs"}
              disabled={saving}
              options={[
                { value: "rel", label: "倒计时" },
                { value: "abs", label: "绝对时间" },
              ]}
              onChange={(value) =>
                set({ dashboardResetTimeRelative: value === "rel" })
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
          <V5Field label="列出全部令牌账户">
            <V5Toggle
              on={settings.showAllTokenAccountsInMenu}
              disabled={saving}
              onChange={(v) => set({ showAllTokenAccountsInMenu: v })}
              label="列出全部令牌账户"
            />
          </V5Field>
        </V5Section>

        <V5Section title="通知区与网格">
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
          className="tray-panel-reveal s5-tray-flyout"
          style={{ transform: `scale(${scaleRatio})` }}
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
              {visible.map((snapshot, idx) => (
                <div key={snapshot.providerId}>
                  {idx > 0 && <div className="provider-stack-divider" />}
                  <TrayCard
                    provider={snapshot}
                    densityMode={density}
                    display={display}
                    showProviderIcon={settings.switcherShowsIcons}
                    localUsagePeriod={settings.localUsagePeriod}
                    outputSpeed={null}
                    detail={isDetail}
                  />
                </div>
              ))}
            </div>
            <footer className="flyout-footer" aria-label={t("PanelMenu")}>

              <button type="button" className="footer-row">
                <span className="footer-row__left">
                  <span className="footer-row__icon">
                    <svg {...footerIconProps}>
                      <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
                      <path d="M21 3v5h-5" />
                      <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
                      <path d="M3 21v-5h5" />
                    </svg>
                  </span>
                  <span className="footer-row__label">{t("ActionRefresh")}</span>
                </span>
                <span className="footer-row__shortcut">Ctrl+R</span>
              </button>
              <button type="button" className="footer-row">
                <span className="footer-row__left">
                  <span className="footer-row__icon">
                    <svg {...footerIconProps}>
                      <circle cx="12" cy="12" r="3" />
                      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0 1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 2-2 2 2 0 0 1 2 2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
                    </svg>
                  </span>
                  <span className="footer-row__label">{t("MenuSettings")}</span>
                </span>
                <span className="footer-row__shortcut">Ctrl+,</span>
              </button>
              <button type="button" className="footer-row">
                <span className="footer-row__left">
                  <span className="footer-row__icon">
                    <svg {...footerIconProps}>
                      <path d="M18.36 6.64a9 9 0 1 1-12.73 0" />
                      <line x1="12" y1="2" x2="12" y2="12" />
                    </svg>
                  </span>
                  <span className="footer-row__label">{t("MenuQuit")}</span>
                </span>
                <span className="footer-row__shortcut">Ctrl+Q</span>
              </button>
            </footer>
          </div>
        </div>
      </SurfacePreviewFrame>
    </div>
  );
}
