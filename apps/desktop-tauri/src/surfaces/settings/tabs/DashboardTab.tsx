import { Field, NumberInput, SegmentedControl, Toggle } from "../../../components/FormControls";
import { useLocale } from "../../../hooks/useLocale";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  TrayIconMode,
} from "../../../types/bridge";
import type { TabProps } from "../../Settings";
import BinaryChoiceField from "../BinaryChoiceField";
import PreviewFrame from "../PreviewFrame";
import TrayPanelPreview from "../previews/TrayPanelPreview";

/**
 * Tray panel settings. File name stays DashboardTab so Lane S can route
 * `trayPanel` / `dashboard` here without a shell rewrite.
 */

const DASHBOARD_DEFAULTS = {
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

export default function DashboardTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const scale = settings.trayScalePercent ?? 100;

  return (
    <div className="settings-surf-page">
      <div className="settings-surf-split">
        <div className="settings-surf-fields">
          <section className="settings-section">
            <div className="settings-section-heading">
              <h3 className="settings-section__title">卡片内容</h3>
              <button
                type="button"
                className="settings-section-heading__action"
                disabled={saving}
                onClick={() => set(DASHBOARD_DEFAULTS)}
              >
                {t("ComponentResetDefaults")}
              </button>
            </div>
            <div className="settings-section__group">
              <Field label={t("DisplayModeLabel")} description={t("DisplayModeHelper")}>
                <SegmentedControl
                  value={settings.menuBarDisplayMode}
                  disabled={saving}
                  options={[
                    { value: "minimal", label: t("DisplayModeMinimal") },
                    { value: "compact", label: t("DisplayModeCompact") },
                    { value: "detailed", label: t("DisplayModeDetailed") },
                  ]}
                  onChange={(value) =>
                    set({ menuBarDisplayMode: value as MenuBarDisplayMode })
                  }
                />
              </Field>
              <Field
                label={t("LocalUsagePeriodLabel")}
                description={t("LocalUsagePeriodHelper")}
              >
                <SegmentedControl
                  value={settings.localUsagePeriod ?? "today"}
                  disabled={saving}
                  options={[
                    { value: "today", label: t("PanelToday") },
                    { value: "7d", label: t("FloatBarSevenDayShort") },
                    { value: "30d", label: t("FloatBarThirtyDayShort") },
                  ]}
                  onChange={(value) =>
                    set({ localUsagePeriod: value as LocalUsagePeriod })
                  }
                />
              </Field>
              <BinaryChoiceField
                label={t("ShowAsUsedLabel")}
                description={t("ShowAsUsedHelper")}
                onLabel={t("QuotaShowUsedOption")}
                offLabel={t("QuotaShowRemainingOption")}
                value={settings.dashboardShowAsUsed}
                disabled={saving}
                onChange={(value) => set({ dashboardShowAsUsed: value })}
              />
              <BinaryChoiceField
                label={t("ResetTimeRelative")}
                description={t("ResetTimeRelativeHelper")}
                onLabel={t("ResetTimeCountdownOption")}
                offLabel={t("ResetTimeAbsoluteOption")}
                value={settings.dashboardResetTimeRelative}
                disabled={saving}
                onChange={(value) => set({ dashboardResetTimeRelative: value })}
              />
              <Field
                label={t("OutputSpeedSettingLabel")}
                description={t("OutputSpeedSettingHelper")}
              >
                <Toggle
                  checked={settings.outputSpeedEnabled !== false}
                  disabled={saving}
                  onChange={(value) => set({ outputSpeedEnabled: value })}
                />
              </Field>
              <Field
                label={t("ShowAllTokenAccountsLabel")}
                description={t("ShowAllTokenAccountsHelper")}
              >
                <Toggle
                  checked={settings.showAllTokenAccountsInMenu}
                  disabled={saving}
                  onChange={(value) => set({ showAllTokenAccountsInMenu: value })}
                />
              </Field>
            </div>
          </section>

          <section className="settings-section">
            <h3 className="settings-section__title">通知区与网格</h3>
            <div className="settings-section__group">
              <Field label={t("TrayIconModeLabel")} description={t("TrayIconModeHelper")}>
                <SegmentedControl
                  value={settings.trayIconMode}
                  disabled={saving}
                  options={[
                    { value: "single", label: t("TrayIconModeSingle") },
                    { value: "perProvider", label: t("TrayIconModePerProvider") },
                  ]}
                  onChange={(value) => set({ trayIconMode: value as TrayIconMode })}
                />
              </Field>
              <Field label={t("ShowProviderIcons")} description={t("ShowProviderIconsHelper")}>
                <Toggle
                  checked={settings.switcherShowsIcons}
                  disabled={saving}
                  onChange={(value) => set({ switcherShowsIcons: value })}
                />
              </Field>
              <BinaryChoiceField
                label={t("PreferHighestUsage")}
                description={t("PreferHighestUsageHelper")}
                onLabel={t("TrayProviderHighestOption")}
                offLabel={t("TrayProviderFirstOption")}
                value={settings.menuBarShowsHighestUsage}
                disabled={saving}
                onChange={(value) => set({ menuBarShowsHighestUsage: value })}
              />
              <Field label={t("PanelZoom")} description="只放大托盘内容">
                <div className="settings-value-with-unit">
                  <NumberInput
                    value={scale}
                    min={100}
                    max={200}
                    step={1}
                    disabled={saving}
                    onChange={(value) => set({ trayScalePercent: value })}
                  />
                  <span>%</span>
                </div>
              </Field>
            </div>
          </section>
        </div>

        <PreviewFrame kind="tray" label="实时预览">
          <TrayPanelPreview settings={settings} />
        </PreviewFrame>
      </div>
    </div>
  );
}
