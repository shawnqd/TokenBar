import { useLocale } from "../../../hooks/useLocale";
import type { ThemePreference } from "../../../types/bridge";
import FontSettingsBlock from "../FontSettingsBlock";
import type { SettingsPageProps } from "./pageTypes";
import { V5Field, V5Section, V5Seg, V5Toggle } from "./v5Controls";

export default function AppearancePage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
  const { t } = useLocale();

  return (
    <>
      <V5Section title="主题与动效">
        <V5Field label={t("ThemeLabel")} help={t("ThemeHelper")}>
          <V5Seg
            value={settings.theme}
            disabled={saving}
            options={[
              { value: "auto", label: t("ThemeAutoOption") },
              { value: "light", label: t("ThemeLightOption") },
              { value: "dark", label: t("ThemeDarkOption") },
            ]}
            onChange={(value) => set({ theme: value as ThemePreference })}
          />
        </V5Field>
        <V5Field
          label={t("EnableAnimationsLabel")}
          help={t("EnableAnimationsHelper")}
        >
          <V5Toggle
            on={settings.enableAnimations}
            disabled={saving}
            onChange={(v) => set({ enableAnimations: v })}
            label={t("EnableAnimationsLabel")}
          />
        </V5Field>
      </V5Section>

      <V5Section title={t("ContextMenuSection")} hint="托盘图标和小型状态栏共用这一套字体。">
        <div className="s5-field" style={{ display: "block" }}>
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
      </V5Section>
    </>
  );
}
