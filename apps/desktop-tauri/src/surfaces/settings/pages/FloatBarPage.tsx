import { useCallback, useEffect, useMemo } from "react";
import { useLocale } from "../../../hooks/useLocale";
import type { LocaleKey } from "../../../i18n/keys";
import type {
  FloatBarOrientation,
  FloatBarResetWindow,
  FloatBarStyle,
  TaskbarEntry,
  TaskbarWindowKind,
} from "../../../types/bridge";
import {
  floatBarIdsFromEntries,
  resolveFloatBarEntries,
} from "../floatBarEntries";
import type { SettingsPageProps } from "./pageTypes";
import FloatBarPreview from "../previews/FloatBarPreview";
import { SurfacePreviewFrame } from "./HtmlSurfacePreviews";
import { V5EntryList } from "./V5EntryList";
import { V5Field, V5Section, V5Seg, V5Toggle } from "./v5Controls";
import { catalogChoices } from "./htmlFixture";
import {
  taskbarWindowLabelFor,
  taskbarWindowOptionsFor,
  useTaskbarWindowAvailability,
} from "../taskbarWindowOptions";
import {
  quotaDisplayContext,
  quotaDisplayPreference,
  resetDisplayPreference,
} from "../../../lib/quotaDisplay";

/** Same vocabulary the float bar runtime uses for inline reset labels
 *  (`FloatBar.tsx` RESET_WINDOW_LABEL_KEYS); "primary"/"session" are internal
 *  compat identifiers and must never surface as user copy. */
const RESET_CHIPS: { value: FloatBarResetWindow; labelKey: LocaleKey }[] = [
  { value: "primary", labelKey: "FloatBarResetWindowPrimary" },
  { value: "session", labelKey: "TaskbarWindowSession" },
  { value: "daily", labelKey: "TaskbarWindowDaily" },
  { value: "weekly", labelKey: "TaskbarWindowWeekly" },
  { value: "monthly", labelKey: "TaskbarWindowMonthly" },
];

function opacityPercent(value: number): number {
  if (value <= 1) return Math.round(value * 100);
  return value;
}

export default function FloatBarPage({
  settings,
  set,
  saving,
  catalog,
  head,
}: SettingsPageProps) {
  const { t, language } = useLocale();
  const enabled = settings.floatBarEnabled;
  const off = !enabled;
  // Runtime consumes floatBarEntries; legacy ids only via resolver
  const entries = resolveFloatBarEntries(settings);

  useEffect(() => {
    // Keep legacy ids in sync when entries change externally (migration observability)
  }, [settings.floatBarEntries, settings.floatBarProviderIds]);

  const providers = useMemo(
    () => catalogChoices(catalog, settings.enabledProviders),
    [catalog, settings.enabledProviders],
  );
  const windowAvailability = useTaskbarWindowAvailability(enabled);
  const windowOptionsFor = useCallback(
    (entry: TaskbarEntry): TaskbarWindowKind[] =>
      taskbarWindowOptionsFor(entry, windowAvailability, {
        preserveSelection: false,
      }),
    [windowAvailability],
  );
  const windowLabelFor = useCallback(
    (kind: TaskbarWindowKind, entry: TaskbarEntry) =>
      taskbarWindowLabelFor(kind, t, language, entry.providerId),
    [language, t],
  );

  const commitEntries = (next: TaskbarEntry[]) => {
    
    set({ floatBarEntries: next, floatBarProviderIds: floatBarIdsFromEntries(next) });
  };

  const opacity = opacityPercent(settings.floatBarOpacity ?? 80);
  const windows = settings.floatBarResetWindows ?? [];
  const quotaMode = quotaDisplayPreference(settings, "floatBar");
  const resetMode = resetDisplayPreference(settings, "floatBar");
  const display = quotaDisplayContext(settings, "floatBar");
  const effectiveQuotaLabel = display.showAsUsed
    ? t("QuotaShowUsedOption")
    : t("QuotaShowRemainingOption");
  const quotaHelp = t(
    quotaMode === "follow" ? "QuotaFollowHelper" : "QuotaOverrideHelper",
  ).replace(
    "{}",
    effectiveQuotaLabel,
  );
  const effectiveResetLabel = display.resetTimeRelative
    ? t("ResetTimeCountdownOption")
    : t("ResetTimeAbsoluteOption");
  const resetHelp = t(
    resetMode === "follow" ? "QuotaFollowHelper" : "QuotaOverrideHelper",
  ).replace(
    "{}",
    effectiveResetLabel,
  );

  return (
    <div className="s5-surf-split">
      <div className="s5-surf-fields">
        {head}
        <V5Section
          title="窗口与交互"
          resetLabel={t("ComponentResetDefaults")}
          resetDisabled={saving}
          onReset={() =>
            set({
              floatBarEnabled: false,
              floatBarOrientation: "horizontal",
              floatBarStyle: "floating",
              floatBarDarkText: false,
              floatBarClickThrough: false,
            })
          }
        >
          <V5Field label="显示悬浮栏" help="在桌面上方常驻显示灵动微胶囊悬浮栏">
            <V5Toggle
              on={enabled}
              disabled={saving}
              onChange={(v) => set({ floatBarEnabled: v })}
              label="显示悬浮栏"
            />
          </V5Field>
          <V5Field label="排列方向" help="桌面停靠排布方向" off={off}>
            <V5Seg
              value={
                settings.floatBarOrientation === "vertical" ? "v" : "h"
              }
              disabled={saving || off}
              options={[
                { value: "h", label: "横向" },
                { value: "v", label: "纵向" },
              ]}
              onChange={(value) =>
                set({
                  floatBarOrientation: (value === "v"
                    ? "vertical"
                    : "horizontal") as FloatBarOrientation,
                })
              }
            />
          </V5Field>
          <V5Field label="窗口样式" help="药丸悬浮或贴边风格" off={off}>
            <V5Seg
              value={settings.floatBarStyle === "taskbar" ? "edge" : "float"}
              disabled={saving || off}
              options={[
                { value: "float", label: "悬浮" },
                { value: "edge", label: "贴边" },
              ]}
              onChange={(value) =>
                set({
                  floatBarStyle: (value === "edge"
                    ? "taskbar"
                    : "floating") as FloatBarStyle,
                })
              }
            />
          </V5Field>
          <V5Field label="浅色桌面自适应" help="在亮色桌面壁纸下增强对比度与文字清晰度" off={off}>
            <V5Toggle
              on={settings.floatBarDarkText}
              disabled={saving || off}
              onChange={(v) => set({ floatBarDarkText: v })}
              label="浅色桌面自适应"
            />
          </V5Field>
          <V5Field
            label="鼠标点击穿透"
            help="开启后鼠标穿透悬浮栏；如需移动或关闭，请在设置中操作"
            off={off}
          >
            <V5Toggle
              on={settings.floatBarClickThrough}
              disabled={saving || off}
              onChange={(v) => set({ floatBarClickThrough: v })}
              label="鼠标点击穿透"
            />
          </V5Field>
        </V5Section>

        <V5Section
          title="显示内容"
          resetLabel={t("ComponentResetDefaults")}
          resetDisabled={saving}
          onReset={() =>
            set({
              floatBarQuotaDisplay: "follow",
              floatBarShowResetInline: false,
              floatBarResetDisplay: "follow",
              floatBarResetWindows: ["primary"],
              floatBarShowCost: false,
            })
          }
        >
          <V5Field
            label="额度展示口径"
            help={quotaHelp}
            off={off}
          >
            <V5Seg
              value={quotaMode}
              disabled={saving || off}
              options={[
                { value: "follow", label: t("QuotaFollowOption") },
                { value: "used", label: t("QuotaShowUsedOption") },
                { value: "remaining", label: t("QuotaShowRemainingOption") },
              ]}
              onChange={(value) =>
                set({ floatBarQuotaDisplay: value as "follow" | "used" | "remaining" })
              }
            />
          </V5Field>
          <V5Field
            label="条内显示重置时间"
            help="在服务商胶囊内直接呈现重置时间或倒计时"
            off={off}
          >
            <V5Toggle
              on={settings.floatBarShowResetInline}
              disabled={saving || off}
              onChange={(v) => set({ floatBarShowResetInline: v })}
              label="条内显示重置时间"
            />
          </V5Field>
          <V5Field
            label="重置时间形式"
            help={resetHelp}
            off={off || !settings.floatBarShowResetInline}
          >
            <V5Seg
              value={resetMode}
              disabled={saving || off || !settings.floatBarShowResetInline}
              options={[
                { value: "follow", label: t("QuotaFollowOption") },
                { value: "countdown", label: t("ResetTimeCountdownOption") },
                { value: "absolute", label: t("ResetTimeAbsoluteOption") },
              ]}
              onChange={(value) =>
                set({ floatBarResetDisplay: value as "follow" | "countdown" | "absolute" })
              }
            />
          </V5Field>
          <V5Field
            label="重置周期窗口"
            help="选择内联重置时间对应的额度周期"
            off={off || !settings.floatBarShowResetInline}
          >
            <div className="s5-chips">
              {RESET_CHIPS.map((chip) => {
                const on = windows.includes(chip.value);
                return (
                  <button
                    key={chip.value}
                    type="button"
                    className={`s5-chip${on ? " on" : ""}`}
                    disabled={saving || off || !settings.floatBarShowResetInline}
                    onClick={() => {
                      const next = on
                        ? windows.filter((item) => item !== chip.value)
                        : [...windows, chip.value];
                      set({ floatBarResetWindows: next });
                    }}
                  >
                    {t(chip.labelKey)}
                  </button>
                );
              })}
            </div>
          </V5Field>
          <V5Field label="显示费用与余额" help="在胶囊旁呈现今日或累计费用/余额" off={off}>
            <V5Toggle
              on={settings.floatBarShowCost ?? false}
              disabled={saving || off}
              onChange={(v) => set({ floatBarShowCost: v })}
              label="显示费用与余额"
            />
          </V5Field>
        </V5Section>

        <V5Section
          title="服务商与额度"
          resetLabel={t("ComponentResetDefaults")}
          resetDisabled={saving}
          onReset={() => {
            set({
              floatBarProviderIds: [],
              floatBarEntries: [],
            });
          }}
          hint="每行选定服务商与额度周期。留空时「跟随」自动按服务商顺序展示。"
        >
          <V5EntryList
            entries={entries}
            providers={providers}
            followLabel="跟随"
            onChange={commitEntries}
            newWindow="session"
            maxEntries={6}
            minEntries={1}
            addLabel="添加条目"
            disabled={saving || off}
            windowOptionsFor={windowOptionsFor}
            windowLabelFor={windowLabelFor}
          />
        </V5Section>

        <V5Section
          title="外观与尺寸"
          resetLabel={t("ComponentResetDefaults")}
          resetDisabled={saving}
          onReset={() =>
            set({
              floatBarOpacity: 80,
              floatBarScale: 100,
            })
          }
        >
          <V5Field label="毛玻璃不透明度" help="30% 到 100%，每格 5%" off={off}>
            <div className="s5-range-row">
              <input
                type="range"
                min={30}
                max={100}
                step={5}
                value={opacity}
                disabled={saving || off}
                onChange={(event) =>
                  set({ floatBarOpacity: Number(event.target.value) })
                }
              />
              <span className="s5-unit">{opacity}%</span>
            </div>
          </V5Field>
          <V5Field label="界面缩放" help="75% 到 200%，每格 5%" off={off}>
            <div className="s5-range-row">
              <input
                type="range"
                min={75}
                max={200}
                step={5}
                value={settings.floatBarScale ?? 100}
                disabled={saving || off}
                onChange={(event) =>
                  set({ floatBarScale: Number(event.target.value) })
                }
              />
              <span className="s5-unit">{settings.floatBarScale ?? 100}%</span>
            </div>
          </V5Field>
        </V5Section>
      </div>

      <SurfacePreviewFrame kind="float">
        <FloatBarPreview settings={settings}
            
            
            
            
            
          
          
          
          
          
        />
      </SurfacePreviewFrame>
    </div>
  );
}
