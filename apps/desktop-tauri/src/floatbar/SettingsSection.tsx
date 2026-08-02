import { useCallback, useEffect, useState } from "react";
import { Field, Select, Toggle } from "../components/FormControls";
import { useLocale } from "../hooks/useLocale";
import type {
  FloatBarOrientation,
  FloatBarResetWindow,
  FloatBarStyle,
  SettingsSnapshot,
  SettingsUpdate,
} from "../types/bridge";
import { FLOAT_BAR_MAX_RESET_WINDOWS } from "../types/bridge";

/** Offered in cycle-length order, shortest first, so the list reads sensibly. */
const RESET_WINDOW_CHOICES: FloatBarResetWindow[] = [
  "primary",
  "session",
  "daily",
  "weekly",
  "monthly",
];

const RESET_WINDOW_LABEL_KEYS = {
  primary: "FloatBarResetWindowPrimary",
  session: "TaskbarWindowSession",
  weekly: "TaskbarWindowWeekly",
  daily: "TaskbarWindowDaily",
  monthly: "TaskbarWindowMonthly",
} as const;

interface Props {
  settings: SettingsSnapshot;
  saving: boolean;
  set: (patch: SettingsUpdate) => void;
}

function useDraftNumber(value: number) {
  const [draft, setDraft] = useState(value);

  useEffect(() => {
    setDraft(value);
  }, [value]);

  const commit = useCallback(
    (next: number, onCommit: (value: number) => void) => {
      // Dedupe against the committed prop value, which is the persisted
      // source of truth. The parent's save is fire-and-forget, so we can't
      // observe success/failure here — comparing to `value` (rather than an
      // optimistically-advanced marker) means a failed save leaves the prop
      // unchanged and a re-commit of the same number still fires the retry.
      if (next === value) return;
      onCommit(next);
    },
    [value],
  );

  return { draft, setDraft, commit };
}

/**
 * Settings UI block for the floating capacity bar. Rendered as one row
 * in the Display tab — kept in this module so the Display tab only
 * imports a single component.
 */
export default function FloatBarSettingsSection({ settings, saving, set }: Props) {
  const { t } = useLocale();
  const opacity = useDraftNumber(settings.floatBarOpacity);
  const scale = useDraftNumber(settings.floatBarScale);
  const commitOpacity = () => {
    opacity.commit(opacity.draft, (value) => set({ floatBarOpacity: value }));
  };
  const commitScale = () => {
    scale.commit(scale.draft, (value) => set({ floatBarScale: value }));
  };
  // The pre-setting behaviour when the key is absent: the provider's own
  // leading window, which is what the bar has always printed.
  const resetWindows = settings.floatBarResetWindows ?? ["primary"];

  return (
    <section className="settings-section">
      <h3 className="settings-section__title">{t("FloatBarSectionTitle")}</h3>
      <div className="settings-section__group">
        <Field
          label={t("FloatBarShowLabel")}
          description={t("FloatBarShowHelper")}
        >
          <Toggle
            checked={settings.floatBarEnabled}
            disabled={saving}
            onChange={(v) => set({ floatBarEnabled: v })}
          />
        </Field>
        <Field
          label={t("FloatBarOrientationLabel")}
          description={t("FloatBarOrientationHelper")}
        >
          <Select
            value={settings.floatBarOrientation}
            disabled={saving || !settings.floatBarEnabled}
            options={[
              { value: "horizontal", label: t("FloatBarOrientationHorizontal") },
              { value: "vertical", label: t("FloatBarOrientationVertical") },
            ]}
            onChange={(v) => set({ floatBarOrientation: v as FloatBarOrientation })}
          />
        </Field>
        <Field
          label={t("FloatBarStyleLabel")}
          description={t("FloatBarStyleHelper")}
        >
          <Select
            value={settings.floatBarStyle}
            disabled={saving || !settings.floatBarEnabled}
            options={[
              { value: "floating", label: t("FloatBarStyleFloating") },
              { value: "taskbar", label: t("FloatBarStyleTaskbar") },
            ]}
            onChange={(v) => set({ floatBarStyle: v as FloatBarStyle })}
          />
        </Field>
        <Field
          label={`${t("FloatBarOpacityLabel")} (${opacity.draft}%)`}
          description={t("FloatBarOpacityHelper")}
        >
          <input
            type="range"
            min={30}
            max={100}
            step={5}
            value={opacity.draft}
            disabled={!settings.floatBarEnabled}
            onChange={(e) => opacity.setDraft(Number(e.target.value))}
            onPointerUp={commitOpacity}
            onTouchEnd={commitOpacity}
            onBlur={commitOpacity}
            onKeyUp={commitOpacity}
            aria-label={t("FloatBarOpacityAriaLabel")}
          />
        </Field>
        <Field
          label={`${t("FloatBarSizeLabel")} (${scale.draft}%)`}
          description={t("FloatBarSizeHelper")}
        >
          <input
            type="range"
            min={75}
            max={200}
            step={5}
            value={scale.draft}
            disabled={!settings.floatBarEnabled}
            onChange={(e) => scale.setDraft(Number(e.target.value))}
            onPointerUp={commitScale}
            onTouchEnd={commitScale}
            onBlur={commitScale}
            onKeyUp={commitScale}
            aria-label={t("FloatBarSizeAriaLabel")}
          />
        </Field>
        <Field
          label={t("FloatBarShowCost")}
          description={t("FloatBarShowCostDescription")}
        >
          <Toggle
            checked={settings.floatBarShowCost ?? false}
            disabled={saving || !settings.floatBarEnabled}
            onChange={(v) => set({ floatBarShowCost: v })}
          />
        </Field>
        <Field
          label={t("FloatBarShowResetInlineLabel")}
          description={t("FloatBarShowResetInlineHelper")}
        >
          <Toggle
            checked={settings.floatBarShowResetInline}
            disabled={saving || !settings.floatBarEnabled}
            onChange={(v) => set({ floatBarShowResetInline: v })}
          />
        </Field>
        <Field
          label={t("FloatBarResetWindowsLabel")}
          description={t("FloatBarResetWindowsHelper")}
        >
          <div className="option-chips" role="group">
            {RESET_WINDOW_CHOICES.map((kind) => {
              const active = resetWindows.includes(kind);
              // At the cap, the unselected chips are the ones that would push
              // past it — disable those rather than silently dropping a pick.
              const atCap =
                !active && resetWindows.length >= FLOAT_BAR_MAX_RESET_WINDOWS;
              return (
                <button
                  key={kind}
                  type="button"
                  className="option-chips__chip"
                  aria-pressed={active}
                  disabled={
                    saving ||
                    !settings.floatBarEnabled ||
                    !settings.floatBarShowResetInline ||
                    atCap
                  }
                  onClick={() =>
                    set({
                      floatBarResetWindows: active
                        ? resetWindows.filter((value) => value !== kind)
                        : [...resetWindows, kind],
                    })
                  }
                >
                  {t(RESET_WINDOW_LABEL_KEYS[kind])}
                </button>
              );
            })}
          </div>
        </Field>
        <Field
          label={t("FloatBarInvertColorsLabel")}
          description={t("FloatBarInvertColorsHelper")}
        >
          <Toggle
            checked={settings.floatBarDarkText}
            disabled={saving || !settings.floatBarEnabled}
            onChange={(v) => set({ floatBarDarkText: v })}
          />
        </Field>
        <Field
          label={t("FloatBarClickThroughLabel")}
          description={t("FloatBarClickThroughHelper")}
        >
          <Toggle
            checked={settings.floatBarClickThrough}
            disabled={saving || !settings.floatBarEnabled}
            onChange={(v) => set({ floatBarClickThrough: v })}
          />
        </Field>
      </div>
    </section>
  );
}
