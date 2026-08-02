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
 */

export default function DisplayTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();

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

    </>
  );
}
