import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocale } from "../../hooks/useLocale";
import { Field, NumberInput, Select, Toggle } from "../../components/FormControls";
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
 * Extracted from `TaskbarTab` when the right-click menu needed the same three
 * controls. Both surfaces are drawn by `taskbar_text.rs`, so they have the same
 * capabilities and deserve the same controls — the alternative was a second,
 * simpler set of menu-only controls, which would have implied the menu's font
 * works differently when it does not.
 *
 * The family list comes from `IDWriteFontCollection` via the backend rather
 * than a hardcoded list, so it can never offer something the machine cannot
 * render.
 */
export default function FontSettingsBlock({ value, disabled, onChange }: Props) {
  const { t } = useLocale();
  const [families, setFamilies] = useState<TaskbarFontFamily[]>([]);
  const [showAllFonts, setShowAllFonts] = useState(false);

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

  // Whether the slider's intermediate positions mean anything depends entirely
  // on the renderer: a static family makes DirectWrite pick the nearest
  // installed face, so in-between values look identical. The helper text says
  // which case the user is in rather than letting them discover it.
  const selectedIsVariable =
    families.find((f) => f.name === value.family)?.variableWeight ?? false;

  // A typical Windows install carries ~400 families, nearly all of them symbol,
  // script and per-app faces. The backend marks a short curated set; everything
  // else stays one switch away rather than being hidden.
  const recommendedFamilies = useMemo(
    () => families.filter((family) => family.recommended),
    [families],
  );

  const fontOptions = useMemo(() => {
    const listed = showAllFonts ? families : recommendedFamilies;
    return [
      // Keep the persisted family selectable even when it is not installed or
      // not curated, so the control never appears to silently reset it.
      ...(listed.some((family) => family.name === value.family)
        ? []
        : [{ value: value.family, label: value.family }]),
      ...listed.map((family) => {
        const tags = [
          family.variableWeight ? t("TaskbarFontVariableTag") : null,
          family.hasCjk ? null : t("TaskbarFontNoCjkTag"),
        ].filter(Boolean);
        return {
          value: family.name,
          label: tags.length ? `${family.name} · ${tags.join(" · ")}` : family.name,
        };
      }),
    ];
  }, [families, recommendedFamilies, value.family, showAllFonts, t]);

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
          disabled={disabled || families.length === 0}
          options={fontOptions}
          onChange={(family) => onChange({ family })}
        />
      </Field>
      <Field
        label={t("TaskbarFontShowAll")}
        description={`${families.length} / ${recommendedFamilies.length}`}
      >
        <Toggle
          checked={showAllFonts}
          disabled={disabled || families.length === 0}
          onChange={setShowAllFonts}
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
