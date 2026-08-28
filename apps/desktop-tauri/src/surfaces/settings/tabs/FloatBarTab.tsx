import { useEffect, useMemo, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { FloatBarSettingsSection } from "../../../floatbar";
import {
  getTaskbarWindowAvailability,
} from "../../../lib/tauri";
import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type { TaskbarEntry, TaskbarWindowKind } from "../../../types/bridge";
import type { TabProps } from "../../Settings";
import BinaryChoiceField from "../BinaryChoiceField";
import {
  resolveFloatBarEntries,
  floatBarIdsFromEntries,
} from "../floatBarEntries";
import PreviewFrame from "../PreviewFrame";
import FloatBarPreview from "../previews/FloatBarPreview";
import TaskbarEntryList from "../TaskbarEntryList";

const WINDOW_KINDS: TaskbarWindowKind[] = [
  "primary",
  "session",
  "weekly",
  "daily",
  "monthly",
  "balance",
  "speed",
];

const WINDOW_LABEL_KEYS = {
  primary: "TaskbarWindowPrimary",
  session: "TaskbarWindowSession",
  weekly: "TaskbarWindowWeekly",
  daily: "TaskbarWindowDaily",
  monthly: "TaskbarWindowMonthly",
  balance: "TaskbarWindowBalance",
  speed: "TaskbarWindowSpeed",
} as const;

const MAX_ENTRIES = 6;

const FLOAT_BAR_CONTENT_DEFAULTS = {
  floatBarShowAsUsed: true,
  floatBarResetTimeRelative: true,
  floatBarProviderIds: [] as string[],
  floatBarEntries: [] as TaskbarEntry[],
};

export default function FloatBarTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const [entries, setEntries] = useState<TaskbarEntry[]>(() =>
    resolveFloatBarEntries(settings),
  );
  const [availability, setAvailability] = useState<Record<
    string,
    TaskbarWindowKind[]
  > | null>(null);

  useEffect(() => {
    setEntries(resolveFloatBarEntries(settings));
  }, [settings.floatBarEntries, settings.floatBarProviderIds]);

  useEffect(() => {
    let cancelled = false;
    getTaskbarWindowAvailability()
      .then((map) => {
        if (!cancelled) setAvailability(map);
      })
      .catch(() => {
        if (!cancelled) setAvailability(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const providerChoices = useMemo(
    () => [
      {
        id: TASKBAR_PROVIDER_AUTO,
        label: "跟随已启用",
      },
      ...(settings.enabledProviders ?? []).map((id) => ({ id, label: id })),
    ],
    [settings.enabledProviders],
  );

  const windowOptionsFor = (entry: TaskbarEntry): TaskbarWindowKind[] => {
    if (!availability) return WINDOW_KINDS;
    const allowed =
      entry.providerId === TASKBAR_PROVIDER_AUTO
        ? new Set(Object.values(availability).flat())
        : new Set(availability[entry.providerId] ?? []);
    const offered = WINDOW_KINDS.filter((kind) => allowed.has(kind));
    if (!offered.includes(entry.window)) offered.push(entry.window);
    return offered;
  };

  const commitEntries = (next: TaskbarEntry[]) => {
    setEntries(next);
    // Runtime consumes floatBarEntries; legacy ids kept for migration
    set({ floatBarEntries: next, floatBarProviderIds: floatBarIdsFromEntries(next) });
  };

  return (
    <div className="settings-surf-page">
      <div className="settings-surf-split">
        <div className="settings-surf-fields">
          <FloatBarSettingsSection settings={settings} saving={saving} set={set} />

          <section className="settings-section">
            <div className="settings-section-heading">
              <h3 className="settings-section__title">显示内容</h3>
              <button
                type="button"
                className="settings-section-heading__action"
                disabled={saving}
                onClick={() => {
                  const resetEntries = resolveFloatBarEntries({ floatBarEntries: [], floatBarProviderIds: [] } as any);
                  setEntries(resetEntries);
                  set(FLOAT_BAR_CONTENT_DEFAULTS);
                }}
              >
                {t("ComponentResetDefaults")}
              </button>
            </div>
            <p className="settings-section__hint">{t("TaskbarEntriesHelper")}</p>
            <div className="settings-section__group">
              <TaskbarEntryList
                entries={entries}
                providerChoices={providerChoices}
                windowOptionsFor={windowOptionsFor}
                windowLabelKeys={WINDOW_LABEL_KEYS}
                newEntryWindow="session"
                maxEntries={MAX_ENTRIES}
                minEntries={1}
                disabled={saving || !settings.floatBarEnabled}
                onChange={commitEntries}
              />
              <BinaryChoiceField
                label={t("ShowAsUsedLabel")}
                description={t("ShowAsUsedHelper")}
                onLabel={t("QuotaShowUsedOption")}
                offLabel={t("QuotaShowRemainingOption")}
                value={settings.floatBarShowAsUsed}
                disabled={saving || !settings.floatBarEnabled}
                onChange={(value) => set({ floatBarShowAsUsed: value })}
              />
              <BinaryChoiceField
                label={t("ResetTimeRelative")}
                description={
                  settings.floatBarShowResetInline
                    ? t("ResetTimeRelativeHelper")
                    : `${t("ResetTimeRelativeHelper")} ${t("FloatBarResetFormatNeedsInline")}`
                }
                onLabel={t("ResetTimeCountdownOption")}
                offLabel={t("ResetTimeAbsoluteOption")}
                value={settings.floatBarResetTimeRelative}
                disabled={saving || !settings.floatBarEnabled}
                onChange={(value) => set({ floatBarResetTimeRelative: value })}
              />
            </div>
          </section>
        </div>

        <PreviewFrame
          kind="float"
          label="实时预览"
          dimmed={!settings.floatBarEnabled}
        >
          <FloatBarPreview settings={settings} />
        </PreviewFrame>
      </div>
    </div>
  );
}
