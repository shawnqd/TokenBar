import { useLocale } from "../../../hooks/useLocale";
import { Field, SegmentedControl } from "../../../components/FormControls";
import FontSettingsBlock from "../FontSettingsBlock";
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
        <p className="settings-section__hint">{t("ContextMenuFontWeightHelper")}</p>
        <div className="settings-section__group">
          {/* The same block the 小型状态栏 page uses, not a menu-specific
              imitation. Both surfaces are drawn by `taskbar_text.rs`, so the
              menu's font has exactly the strip's capabilities — including the
              variable `wght` axis, which is why the weight control is a real
              slider here rather than a few named stops. */}
          <FontSettingsBlock
            value={{
              size: settings.menuFontSize ?? 12,
              family: settings.menuFontFamily || "Microsoft YaHei UI",
              weight: settings.menuFontWeight ?? 300,
            }}
            disabled={saving}
            onChange={(patch) =>
              set({
                ...(patch.size !== undefined ? { menuFontSize: patch.size } : {}),
                ...(patch.family !== undefined
                  ? { menuFontFamily: patch.family }
                  : {}),
                ...(patch.weight !== undefined
                  ? { menuFontWeight: patch.weight }
                  : {}),
              })
            }
          />
        </div>
      </section>
    </>
  );
}
