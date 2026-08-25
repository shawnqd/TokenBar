import { useCallback, useEffect, useState } from "react";
import { useFontPicker } from "../../hooks/useFontPicker";
import { useLocale } from "../../hooks/useLocale";
import { Field, NumberInput, Select } from "../../components/FormControls";
import FontInstallDialog from "./FontInstallDialog";

export const MIN_FONT_WEIGHT = 100;
export const MAX_FONT_WEIGHT = 1000;

export function normalizeFontWeight(value: number): number {
  if (!Number.isFinite(value)) return 400;
  return Math.min(MAX_FONT_WEIGHT, Math.max(MIN_FONT_WEIGHT, Math.round(value)));
}

export interface FontSettingsValue {
  size: number;
  family: string;
  weight: number;
}

interface Props {
  value: FontSettingsValue;
  disabled?: boolean;
  onChange: (patch: Partial<FontSettingsValue>) => void;
}

/**
 * Size / family / weight for one DirectWrite-rendered surface.
 *
 * The picker is the design-system five-font whitelist only. A previously
 * saved family outside that list stays selectable so the control never
 * silently rewrites a persisted setting.
 */
export default function FontSettingsBlock({ value, disabled, onChange }: Props) {
  const { t } = useLocale();
  const {
    options: fontOptions,
    isContinuous: selectedIsVariable,
    install,
    installMiss,
    chooseFamily,
    cancelInstall,
    openInstallPage,
    confirmInstalled,
  } = useFontPicker(value.family);

  // The slider is driven from a draft and committed on release. Persisting on
  // every `input` event would fire a settings write per pixel dragged.
  const [weightDraft, setWeightDraft] = useState(value.weight);
  useEffect(() => {
    setWeightDraft(value.weight);
  }, [value.weight]);
  const commitWeight = useCallback(() => {
    const next = normalizeFontWeight(weightDraft);
    if (next !== value.weight) {
      onChange({ weight: next });
    }
  }, [onChange, value.weight, weightDraft]);

  return (
    <>
      <Field
        label={t("TaskbarWidgetFontSizeLabel")}
        description={t("TaskbarWidgetFontSizeHelper")}
      >
        <div className="settings-value-with-unit">
          <NumberInput
            value={value.size}
            min={10}
            max={16}
            step={1}
            disabled={disabled}
            onChange={(size) => onChange({ size })}
          />
          <span>px</span>
        </div>
      </Field>
      <Field
        label={t("TaskbarWidgetFontFamilyLabel")}
        description={t("TaskbarWidgetFontFamilyHelper")}
      >
        <Select
          value={value.family}
          disabled={disabled}
          options={fontOptions}
          onChange={(family) => chooseFamily(family, (next) => onChange({ family: next }))}
        />
        {install ? (
          <FontInstallDialog
            guide={install}
            miss={installMiss}
            onCancel={cancelInstall}
            onOpenDownload={openInstallPage}
            onRecheck={() => void confirmInstalled((next) => onChange({ family: next }))}
          />
        ) : null}
      </Field>
      <Field
        label={`${t("TaskbarWidgetFontWeightLabel")} (${weightDraft})`}
        description={
          selectedIsVariable
            ? t("TaskbarWidgetFontWeightHelperVariable")
            : t("TaskbarWidgetFontWeightHelperStatic")
        }
      >
        <div className="taskbar-weight-control">
          <input
            type="range"
            min={MIN_FONT_WEIGHT}
            max={MAX_FONT_WEIGHT}
            step={10}
            value={weightDraft}
            aria-label={t("TaskbarWidgetFontWeightAriaLabel")}
            aria-valuetext={`${weightDraft}`}
            disabled={disabled}
            onChange={(event) =>
              setWeightDraft(normalizeFontWeight(Number(event.target.value)))
            }
            onPointerUp={commitWeight}
            onTouchEnd={commitWeight}
            onBlur={commitWeight}
            onKeyUp={commitWeight}
          />
          <div className="taskbar-weight-control__scale" aria-hidden>
            <span>
              {t("TaskbarWidgetFontWeightLight")} · {MIN_FONT_WEIGHT}
            </span>
            <output>{weightDraft}</output>
            <span>
              {MAX_FONT_WEIGHT} · {t("TaskbarWidgetFontWeightHeavy")}
            </span>
          </div>
        </div>
      </Field>
    </>
  );
}
