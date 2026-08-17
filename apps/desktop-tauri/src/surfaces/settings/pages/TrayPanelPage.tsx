import { useLocale } from "../../../hooks/useLocale";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  TrayIconMode,
} from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import TrayCard from "../../../surfaces/tray/TrayCard";
import { SurfacePreviewFrame } from "./HtmlSurfacePreviews";
import { catalogPreviewSnapshots } from "../previews/catalogFixtures";
import { quotaDisplayContext } from "../../../lib/quotaDisplay";
import { V5Field, V5Num, V5Section, V5Seg, V5Toggle } from "./v5Controls";

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

  const preview = catalogPreviewSnapshots().filter((row) =>
    PREVIEW_PROVIDERS.includes(row.providerId),
  );
  const display = quotaDisplayContext(settings, "dashboard");
  const density = settings.menuBarDisplayMode;

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
        <div className="s5-tray-stack">
          {preview.map((snapshot) => (
            <TrayCard
              key={snapshot.providerId}
              provider={snapshot}
              densityMode={density}
              display={display}
              showProviderIcon={settings.switcherShowsIcons}
              localUsagePeriod={settings.localUsagePeriod}
              outputSpeed={null}
            />
          ))}
        </div>
      </SurfacePreviewFrame>
    </div>
  );
}
