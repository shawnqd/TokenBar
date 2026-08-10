import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocale } from "../../hooks/useLocale";
import { Field, NumberInput, Select } from "../../components/FormControls";
import { getTaskbarFontFamilies } from "../../lib/tauri";
import type { TaskbarFontFamily } from "../../types/bridge";

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
 * The family list is restricted to fonts with a genuine continuous `wght`
 * axis (MiSans, Bahnschrift, Segoe UI Variable, …). Static multi-face
 * families like Microsoft YaHei look continuous on a stepped slider but
 * only snap between installed faces — the backend marks them
 * `variableWeight: false` and they stay out of this picker.
 *
 * A previously-saved static family remains selectable (so the control never
 * silently rewrites a persisted setting) but the helper text still says it
 * is not continuous.
 */
export default function FontSettingsBlock({ value, disabled, onChange }: Props) {
  const { t } = useLocale();
  const [families, setFamilies] = useState<TaskbarFontFamily[]>([]);

  useEffect(() => {
    let cancelled = false;
    getTaskbarFontFamilies()
      .then((list) => {
        if (!cancelled) setFamilies(list);
      })
      .catch(() => {
        // A failed enumeration leaves the persisted family selectable on its
        // own (see `fontOptions`), which is better than an empty dropdown.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const variableFamilies = useMemo(
    () => families.filter((family) => family.variableWeight),
    [families],
  );

  const selectedIsVariable =
    families.find((f) => f.name === value.family)?.variableWeight ?? false;

  const fontOptions = useMemo(() => {
    return [
      // Keep a previously-saved static family selectable so the control never
      // appears to silently reset it — the weight helper still tells the truth.
      ...(variableFamilies.some((family) => family.name === value.family) ||
      !value.family
        ? []
        : [{ value: value.family, label: value.family }]),
      ...variableFamilies.map((family) => {
        const tags = [
          t("TaskbarFontVariableTag"),
          family.hasCjk ? null : t("TaskbarFontNoCjkTag"),
        ].filter(Boolean);
        return {
          value: family.name,
          label: tags.length ? `${family.name} · ${tags.join(" · ")}` : family.name,
        };
      }),
    ];
  }, [variableFamilies, value.family, t]);

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
          disabled={disabled || (families.length === 0 && !value.family)}
          options={fontOptions}
          onChange={(family) => onChange({ family })}
        />
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
