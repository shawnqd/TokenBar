import { useLocale } from "../../../hooks/useLocale";
import { Field, SegmentedControl, Toggle } from "../../../components/FormControls";
import { FONT_WHITELIST_DEFAULT } from "../../../lib/fontWhitelist";
import FontSettingsBlock from "../FontSettingsBlock";
import type { ThemePreference } from "../../../types/bridge";
import type { TabProps } from "../../Settings";

export default function AppearanceTab({ settings, set, saving }: TabProps) {
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
        <h3 className="settings-section__title">{t("Animations")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("EnableAnimationsLabel")}
            description={t("EnableAnimationsHelper")}
          >
            <Toggle
              checked={settings.enableAnimations}
              disabled={saving}
              onChange={(v) => set({ enableAnimations: v })}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title">{t("ContextMenuSection")}</h3>
        <p className="settings-section__hint">{t("ContextMenuFontWeightHelper")}</p>
        <div className="settings-section__group">
          <FontSettingsBlock
            value={{
              size: settings.menuFontSize ?? 12,
              family: settings.menuFontFamily || FONT_WHITELIST_DEFAULT,
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
