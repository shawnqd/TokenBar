import { useLocale } from "../../../hooks/useLocale";
import { Field, SegmentedControl } from "../../../components/FormControls";
import type { ThemePreference } from "../../../types/bridge";
import type { TabProps } from "../../Settings";

/**
 * Application-level appearance only.
 *
 * Item H moved everything component-specific out of here: dashboard presentation
 * to `DashboardTab`, the floating bar to `FloatBarTab`, the strip to
 * `TaskbarTab`. What stays is the app-wide theme, which genuinely has no owning
 * component. Do not reintroduce a component's setting on this page — a single
 * global control quietly overriding three components is exactly what item H
 * exists to prevent.
 *
 * The right-click menu section is not an exception to that rule, it is the rule
 * applied: the menu is a surface with no page of its own, and after M3 it is
 * shared by the mini status bar *and* the tray icon, so putting its controls on
 * either component's page would imply an ownership that does not exist. If the
 * menu ever gets its own tab, move this there.
 */

export default function DisplayTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  // Snap a persisted value to the nearest offered stop, so a weight written by
  // an older build (or by hand) still selects a segment instead of leaving the
  // control blank.
  const menuWeight = [300, 400, 700].reduce((best, stop) =>
    Math.abs(stop - (settings.menuFontWeight ?? 300)) <
    Math.abs(best - (settings.menuFontWeight ?? 300))
      ? stop
      : best,
  );

  return (
    <>
      <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionTheme")}</h3>
        <div className="settings-section__group">
          <Field label={t("ThemeLabel")} description={t("ThemeHelper")}>
            <SegmentedControl
              value={settings.theme}
              disabled={saving}
              options={[
                { value: "auto", label: t("ThemeAutoOption") },
                { value: "light", label: t("ThemeLightOption") },
                { value: "dark", label: t("ThemeDarkOption") },
              ]}
              onChange={(value) => set({ theme: value as ThemePreference })}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title">{t("ContextMenuSection")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("TaskbarWidgetFontWeightLabel")}
            description={t("ContextMenuFontWeightHelper")}
          >
            {/* Three stops, not a slider. The menu is drawn with GDI, which
                has exactly three faces to offer here — Light, Regular and a
                synthesised Bold — so a continuous control would imply
                gradations that do not render. The strip's own weight *is* a
                slider because it goes through DirectWrite's variable axis. */}
            <SegmentedControl
              value={String(menuWeight)}
              disabled={saving}
              options={[
                { value: "300", label: t("TaskbarWidgetFontWeightLight") },
                { value: "400", label: t("FontWeightRegular") },
                { value: "700", label: t("TaskbarWidgetFontWeightHeavy") },
              ]}
              onChange={(value) => set({ menuFontWeight: Number(value) })}
            />
          </Field>
        </div>
      </section>
    </>
  );
}
