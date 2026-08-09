import { useCallback, useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { Field, SegmentedControl, Toggle } from "../../../components/FormControls";
import type {
  LocalUsagePeriod,
  MenuBarDisplayMode,
  TrayIconMode,
} from "../../../types/bridge";
import type { TabProps } from "../../Settings";
import BinaryChoiceField from "../BinaryChoiceField";
import MultiSelectField from "../MultiSelectField";

/**
 * Dashboard settings — the tray flyout and the pop-out panel.
 *
 * Item H splits the settings surface by component. Everything here writes only
 * `dashboard*` keys; the floating bar and the taskbar strip own their own copies
 * of the same choices on their own pages. That separation is the point: changing
 * how the dashboard reads a quota must not silently change the taskbar.
 */

/**
 * The dated cycles a card can show, in increasing length.
 *
 * Deliberately only the cycles — a window with no cycle (a prepaid balance, an
 * API-key status) is never filtered by this control, so offering a chip for it
 * would promise something the filter does not do. See
 * `dashboardShowsQuotaWindow`.
 */
const QUOTA_WINDOW_KINDS = ["session", "daily", "weekly", "monthly"] as const;

/** Reuses the strip composer's labels so one cycle reads the same everywhere. */
const QUOTA_WINDOW_LABEL_KEYS = {
  session: "TaskbarWindowSession",
  daily: "TaskbarWindowDaily",
  weekly: "TaskbarWindowWeekly",
  monthly: "TaskbarWindowMonthly",
} as const;

function clampWindowScalePercent(value: number): number {
  return Math.min(250, Math.max(100, Number.isFinite(value) ? value : 100));
}

/** Only the keys this page owns, so "restore defaults" cannot reach another component. */
const DASHBOARD_DEFAULTS = {
  menuBarDisplayMode: "detailed" as MenuBarDisplayMode,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  showAllTokenAccountsInMenu: false,
  // Empty is "every enabled provider", so restoring defaults widens the
  // dashboard back out rather than pinning today's roster.
  dashboardProviderIds: [] as string[],
  dashboardQuotaWindows: [] as string[],
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
          {/* Leads the group: which providers appear is a bigger question than
              how each one is phrased, and the rows below all describe cards
              that this list decides the existence of. */}
          <MultiSelectField
            label={t("DashboardProvidersLabel")}
            description={t("DashboardProvidersHelper")}
            options={(settings.enabledProviders ?? []).map((id) => ({
              value: id,
              label: id,
            }))}
            value={settings.dashboardProviderIds ?? []}
            allLabel={t("DashboardFilterAll")}
            disabled={saving}
            onChange={(next) => set({ dashboardProviderIds: next })}
          />
          {/* Item H's other dashboard list: which reset cycles the cards show.
              Same control and the same "empty means all" encoding. */}
          <MultiSelectField
            label={t("DashboardQuotaWindowsLabel")}
            description={t("DashboardQuotaWindowsHelper")}
            options={QUOTA_WINDOW_KINDS.map((kind) => ({
              value: kind,
              label: t(QUOTA_WINDOW_LABEL_KEYS[kind]),
            }))}
            value={settings.dashboardQuotaWindows ?? []}
            allLabel={t("DashboardFilterAll")}
            disabled={saving}
            onChange={(next) => set({ dashboardQuotaWindows: next })}
          />
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
