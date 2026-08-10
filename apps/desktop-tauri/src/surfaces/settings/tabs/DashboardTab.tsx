import { useLocale } from "../../../hooks/useLocale";
import { Field, SegmentedControl, Toggle } from "../../../components/FormControls";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  TrayIconMode,
} from "../../../types/bridge";
import type { TabProps } from "../../Settings";
import BinaryChoiceField from "../BinaryChoiceField";

/**
 * Dashboard settings — the tray flyout and the pop-out panel.
 *
 * This page owns only dashboard presentation choices. Provider visibility and
 * quota-window selection are intentionally adaptive: the dashboard follows
 * the provider switches and renders the windows each provider supplies.
 */

/** Only the keys this page owns, so "restore defaults" cannot reach another component. */
const DASHBOARD_DEFAULTS = {
  menuBarDisplayMode: "detailed" as MenuBarDisplayMode,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  showAllTokenAccountsInMenu: false,
  localUsagePeriod: "today" as LocalUsagePeriod,
};

export default function DashboardTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();

  return (
    <>
      <header className="settings-page-head">
        <div>
          <h2 className="settings-page-head__title">{t("TabDashboard")}</h2>
          <p className="settings-page-head__description">
            {t("DashboardSettingsDescription")}
          </p>
        </div>
      </header>

      <section className="settings-section">
        <div className="settings-section-heading">
          <h3 className="settings-section__title">{t("SectionMenuContent")}</h3>
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
        <h3 className="settings-section__title">{t("TrayIconSection")}</h3>
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
        </div>
      </section>
    </>
  );
}
