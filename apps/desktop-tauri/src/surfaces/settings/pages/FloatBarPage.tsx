import { useCallback, useEffect, useMemo } from "react";
import { useLocale } from "../../../hooks/useLocale";
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

const RESET_CHIPS: { value: FloatBarResetWindow; label: string }[] = [
  { value: "primary", label: "主窗口" },
  { value: "session", label: "会话" },
  { value: "daily", label: "日" },
  { value: "weekly", label: "周" },
  { value: "monthly", label: "月" },
];

function opacityPercent(value: number): number {
  if (value <= 1) return Math.round(value * 100);
  return value;
}

export default function FloatBarPage({
  settings,
  set,
  saving,
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
    () => catalogChoices(settings.enabledProviders),
    [settings.enabledProviders],
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
    (kind: TaskbarWindowKind) => taskbarWindowLabelFor(kind, t, language),
    [language, t],
  );

  const commitEntries = (next: TaskbarEntry[]) => {
    
    set({ floatBarEntries: next, floatBarProviderIds: floatBarIdsFromEntries(next) });
  };

  const opacity = opacityPercent(settings.floatBarOpacity ?? 80);
  const windows = settings.floatBarResetWindows ?? [];

  return (
    <div className="s5-surf-split">
      <div className="s5-surf-fields">
        <V5Section
          title="窗口"
          resetLabel={t("ComponentResetDefaults")}
          resetDisabled={saving}
          onReset={() =>
            set({
              floatBarEnabled: false,
              floatBarOrientation: "horizontal",
              floatBarStyle: "floating",
              floatBarOpacity: 80,
              floatBarScale: 100,
              floatBarShowCost: false,
              floatBarShowResetInline: false,
              floatBarResetWindows: ["primary"],
              floatBarDarkText: false,
              floatBarClickThrough: false,
            })
          }
        >
          <V5Field label="显示悬浮栏">
            <V5Toggle
              on={enabled}
              disabled={saving}
              onChange={(v) => set({ floatBarEnabled: v })}
              label="显示悬浮栏"
            />
          </V5Field>
          <V5Field label="方向" help="两态有名，不用下拉" off={off}>
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
          <V5Field label="样式" off={off}>
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
          <V5Field label="不透明度" help="30% 到 100%，每格 5%" off={off}>
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
          <V5Field label="缩放" help="75% 到 200%，每格 5%" off={off}>
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
          <V5Field label="显示费用" help="没有费用数据时格子留空" off={off}>
            <V5Toggle
              on={settings.floatBarShowCost ?? false}
              disabled={saving || off}
              onChange={(v) => set({ floatBarShowCost: v })}
              label="显示费用"
            />
          </V5Field>
          <V5Field
            label="条内显示重置"
            help="关掉后重置时间只出现在悬停提示里"
            off={off}
          >
            <V5Toggle
              on={settings.floatBarShowResetInline}
              disabled={saving || off}
              onChange={(v) => set({ floatBarShowResetInline: v })}
              label="条内显示重置"
            />
          </V5Field>
          <V5Field
            label="重置窗口"
            help="主窗口、会话、日、周、月，有数量上限"
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
                    {chip.label}
                  </button>
                );
              })}
            </div>
          </V5Field>
          <V5Field label="深色文字" help="浅色桌面背景时把字改成深色" off={off}>
            <V5Toggle
              on={settings.floatBarDarkText}
              disabled={saving || off}
              onChange={(v) => set({ floatBarDarkText: v })}
              label="深色文字"
            />
          </V5Field>
          <V5Field
            label="点击穿透"
            help="打开后点不到条子，只能回设置关掉"
            off={off}
          >
            <V5Toggle
              on={settings.floatBarClickThrough}
              disabled={saving || off}
              onChange={(v) => set({ floatBarClickThrough: v })}
              label="点击穿透"
            />
          </V5Field>
        </V5Section>

        <V5Section
          title="显示内容"
          resetLabel={t("ComponentResetDefaults")}
          resetDisabled={saving}
          onReset={() => {
            
            set({
              floatBarShowAsUsed: true,
              floatBarResetTimeRelative: true,
              floatBarProviderIds: [],
                floatBarEntries: [],
            });
          }}
          hint="和小型状态栏同一套：每行选服务商和窗口。至少 1 条。空的「跟随已启用」表示按服务商页的顺序带上已打开的商。"
        >
          <V5EntryList
            entries={entries}
            providers={providers}
            followLabel="跟随已启用"
            onChange={commitEntries}
            newWindow="session"
            maxEntries={6}
            minEntries={1}
            addLabel="添加条目"
            disabled={saving || off}
            windowOptionsFor={windowOptionsFor}
            windowLabelFor={windowLabelFor}
          />
          <V5Field label="额度数字" off={off}>
            <V5Seg
              value={settings.floatBarShowAsUsed ? "used" : "remain"}
              disabled={saving || off}
              options={[
                { value: "used", label: "已用" },
                { value: "remain", label: "剩余" },
              ]}
              onChange={(value) =>
                set({ floatBarShowAsUsed: value === "used" })
              }
            />
          </V5Field>
          <V5Field
            label="重置时间"
            help="条子里不显示重置时，仍用在悬停提示里"
            off={off}
          >
            <V5Seg
              value={settings.floatBarResetTimeRelative ? "rel" : "abs"}
              disabled={saving || off}
              options={[
                { value: "rel", label: "倒计时" },
                { value: "abs", label: "绝对时间" },
              ]}
              onChange={(value) =>
                set({ floatBarResetTimeRelative: value === "rel" })
              }
            />
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
