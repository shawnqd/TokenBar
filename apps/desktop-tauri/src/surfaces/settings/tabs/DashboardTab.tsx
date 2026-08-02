import { useCallback, useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { Field, SegmentedControl, Toggle } from "../../../components/FormControls";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  TrayIconMode,
} from "../../../types/bridge";
import type { TabProps } from "../../Settings";

/**
 * Dashboard settings — the tray flyout and the pop-out panel.
 *
 * Item H splits the settings surface by component. Everything here writes only
 * `dashboard*` keys; the floating bar and the taskbar strip own their own copies
 * of the same choices on their own pages. That separation is the point: changing
 * how the dashboard reads a quota must not silently change the taskbar.
 */

function clampWindowScalePercent(value: number): number {
  return Math.min(250, Math.max(100, Number.isFinite(value) ? value : 100));
}

/** Only the keys this page owns, so "restore defaults" cannot reach another component. */
const DASHBOARD_DEFAULTS = {
  menuBarDisplayMode: "detailed" as MenuBarDisplayMode,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  showAllTokenAccountsInMenu: false,
  localUsagePeriod: "today" as LocalUsagePeriod,
  windowScalePercent: 100,
  trayScalePercent: 100,
};

export default function DashboardTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const [windowScaleDraft, setWindowScaleDraft] = useState(() =>
    clampWindowScalePercent(settings.windowScalePercent),
  );

  useEffect(() => {
    setWindowScaleDraft(clampWindowScalePercent(settings.windowScalePercent));
  }, [settings.windowScalePercent]);

  const commitWindowScale = useCallback(() => {
    const next = clampWindowScalePercent(windowScaleDraft);
    if (next !== settings.windowScalePercent) {
      set({ windowScalePercent: next });
    }
  }, [set, settings.windowScalePercent, windowScaleDraft]);

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
          <Field label={t("ShowAsUsedLabel")} description={t("ShowAsUsedHelper")}>
            <Toggle
              checked={settings.dashboardShowAsUsed}
              disabled={saving}
              onChange={(value) => set({ dashboardShowAsUsed: value })}
            />
          </Field>
          <Field
            label={t("ResetTimeRelative")}
            description={t("ResetTimeRelativeHelper")}
          >
            <Toggle
              checked={settings.dashboardResetTimeRelative}
              disabled={saving}
              onChange={(value) => set({ dashboardResetTimeRelative: value })}
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
          <Field label={t("WindowScaleLabel")} description={t("WindowScaleHelper")}>
            <input
              type="range"
              min={100}
              max={250}
              step={5}
              value={windowScaleDraft}
              disabled={saving}
              onChange={(event) =>
                setWindowScaleDraft(
                  clampWindowScalePercent(Number(event.target.value)),
                )
              }
              onPointerUp={commitWindowScale}
              onTouchEnd={commitWindowScale}
              onBlur={commitWindowScale}
              onKeyUp={commitWindowScale}
              aria-label={t("WindowScaleAriaLabel")}
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
          <Field
            label={t("PreferHighestUsage")}
            description={t("PreferHighestUsageHelper")}
          >
            <Toggle
              checked={settings.menuBarShowsHighestUsage}
              disabled={saving}
              onChange={(value) => set({ menuBarShowsHighestUsage: value })}
            />
          </Field>
        </div>
      </section>
    </>
  );
}
