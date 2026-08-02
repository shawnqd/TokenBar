import { useLocale } from "../../../hooks/useLocale";
import { Field, Toggle } from "../../../components/FormControls";
import { FloatBarSettingsSection } from "../../../floatbar";
import type { TabProps } from "../../Settings";

/**
 * Floating bar settings.
 *
 * Item H gives each component its own page and its own persisted keys. The
 * used-versus-remaining and reset-time choices here write `floatBar*` only —
 * before this page existed those two settings had no control at all and could
 * be changed only by migration from the retired global pair.
 */

/** Only this component's keys, so restoring defaults cannot touch another page. */
const FLOAT_BAR_DEFAULTS = {
  floatBarShowAsUsed: true,
  floatBarResetTimeRelative: true,
  floatBarOpacity: 80,
  floatBarScale: 100,
  floatBarOrientation: "horizontal" as const,
  floatBarStyle: "floating" as const,
  floatBarClickThrough: false,
  floatBarDarkText: false,
  floatBarShowResetInline: false,
  floatBarResetWindows: ["primary" as const],
  floatBarShowCost: false,
};

export default function FloatBarTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();

  return (
    <>
      <header className="settings-page-head">
        <div>
          <h2 className="settings-page-head__title">{t("TabFloatBar")}</h2>
          <p className="settings-page-head__description">
            {t("FloatBarSettingsDescription")}
          </p>
        </div>
      </header>

      {/* The bar's own section leads, because its first row is the master
          "show the floating bar" switch. With the usage-display group on top
          the page opened with two toggles governing a surface that had not been
          switched on yet, several screens above the switch that turns it on —
          which reads as settings for some other surface entirely. */}
      <FloatBarSettingsSection settings={settings} saving={saving} set={set} />

      <section className="settings-section">
        <div className="settings-section-heading">
          <h3 className="settings-section__title">{t("UsageDisplay")}</h3>
          <button
            type="button"
            className="settings-section-heading__action"
            disabled={saving}
            onClick={() => set(FLOAT_BAR_DEFAULTS)}
          >
            {t("ComponentResetDefaults")}
          </button>
        </div>
        <div className="settings-section__group">
          <Field label={t("ShowAsUsedLabel")} description={t("ShowAsUsedHelper")}>
            <Toggle
              checked={settings.floatBarShowAsUsed}
              disabled={saving}
              onChange={(value) => set({ floatBarShowAsUsed: value })}
            />
          </Field>
          {/* Unlike the dashboard, the bar prints a reset only when
              `floatBarShowResetInline` is on — and that toggle lives in the
              section below, so with it off this one appears to do nothing. It
              is not inert: the pill's hover tooltip always carries the reset.
              Say so rather than disabling the row, which would be a lie. */}
          <Field
            label={t("ResetTimeRelative")}
            description={
              settings.floatBarShowResetInline
                ? t("ResetTimeRelativeHelper")
                : `${t("ResetTimeRelativeHelper")} ${t("FloatBarResetFormatNeedsInline")}`
            }
          >
            <Toggle
              checked={settings.floatBarResetTimeRelative}
              disabled={saving}
              onChange={(value) => set({ floatBarResetTimeRelative: value })}
            />
          </Field>
        </div>
      </section>
    </>
  );
}
