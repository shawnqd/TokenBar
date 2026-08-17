import { useEffect, useMemo, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import {
  getTaskbarFontFamilies,
  getTaskbarPreviewLines,
} from "../../../lib/tauri";
import type {
  TaskbarEntry,
  TaskbarFontFamily,
  TaskbarStripCell,
  TaskbarWidgetPosition,
  TaskbarWidgetTextAlign,
} from "../../../types/bridge";
import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import TaskbarStripPreview from "../previews/TaskbarStripPreview";
import { SurfacePreviewFrame, type HtmlIconStyle } from "./HtmlSurfacePreviews";
import { catalogChoices } from "./htmlFixture";
import { V5EntryList } from "./V5EntryList";
import { V5Field, V5Num, V5Section, V5Seg, V5Toggle } from "./v5Controls";

const DEFAULT_ENTRIES: TaskbarEntry[] = [
  { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
  { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" },
];

export default function TaskbarStatusPage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
  const { t } = useLocale();
  const enabled = settings.taskbarWidgetEnabled;
  const off = !enabled;
  const iconSize = settings.taskbarWidgetIconSize ?? 14;
  const iconStyle = (settings.taskbarWidgetIconStyle ?? "pure") as HtmlIconStyle;
  const [weightDraft, setWeightDraft] = useState(
    settings.taskbarWidgetFontWeight ?? 400,
  );
  const [previewLines, setPreviewLines] = useState<TaskbarStripCell[]>([]);
  const [families, setFamilies] = useState<TaskbarFontFamily[]>([]);
  const entries = settings.taskbarWidgetEntries ?? DEFAULT_ENTRIES;
  const width = settings.taskbarWidgetWidth ?? 136;

  useEffect(() => {
    getTaskbarFontFamilies()
      .then(setFamilies)
      .catch(() => {});
  }, []);

  useEffect(() => {
    let cancelled = false;
    getTaskbarPreviewLines()
      .then((lines) => {
        if (!cancelled) setPreviewLines(lines);
      })
      .catch(() => {
        if (!cancelled) setPreviewLines([]);
      });
    return () => {
      cancelled = true;
    };
  }, [
      enabled,
      entries,
      width,
      iconSize,
      iconStyle,
      settings.taskbarWidgetFontSize,
      settings.taskbarWidgetFontWeight,
      settings.taskbarWidgetFontFamily,
      settings.taskbarWidgetTextAlign,
      settings.taskbarShowAsUsed,
    ]);

  useEffect(() => {
    setWeightDraft(settings.taskbarWidgetFontWeight ?? 400);
  }, [settings.taskbarWidgetFontWeight]);

  const providers = useMemo(
    () => catalogChoices(settings.enabledProviders),
    [settings.enabledProviders],
  );

    const tooltip = settings.taskbarTooltipEntries ?? [];
  
  return (
    <div className="s5-surf-split">
      <div className="s5-surf-fields">
        <V5Section title="开关与位置">
          <V5Field label="显示小型状态栏">
            <V5Toggle
              on={enabled}
              disabled={saving}
              onChange={(v) => set({ taskbarWidgetEnabled: v })}
              label="显示小型状态栏"
            />
          </V5Field>
          <V5Field label="用量显示为" off={off}>
            <V5Seg
              value={settings.taskbarShowAsUsed ? "used" : "remain"}
              disabled={saving || off}
              options={[
                { value: "used", label: "已用" },
                { value: "remain", label: "剩余" },
              ]}
              onChange={(value) => set({ taskbarShowAsUsed: value === "used" })}
            />
          </V5Field>
          <V5Field
            label="重置时间显示为"
            help="只影响右键菜单状态行，条带正文不印重置"
            off={off}
          >
            <V5Seg
              value={settings.taskbarResetTimeRelative ? "rel" : "abs"}
              disabled={saving || off}
              options={[
                { value: "rel", label: "倒计时" },
                { value: "abs", label: "具体时间" },
              ]}
              onChange={(value) =>
                set({ taskbarResetTimeRelative: value === "rel" })
              }
            />
          </V5Field>
          <V5Field label="显示位置" off={off}>
            <V5Seg
              value={settings.taskbarWidgetPosition}
              disabled={saving || off}
              options={[
                { value: "left", label: "左侧" },
                { value: "notification", label: "通知区一侧" },
              ]}
              onChange={(value) =>
                set({
                  taskbarWidgetPosition: value as TaskbarWidgetPosition,
                })
              }
            />
          </V5Field>
        </V5Section>

        <V5Section
          title="悬停内容"
          resetLabel="恢复悬停默认"
          resetDisabled={saving || off}
          onReset={() => set({ taskbarTooltipEntries: [] })}
          hint="留空则悬停跟条带一样。"
        >
          <V5EntryList
            entries={tooltip}
            providers={providers}
            followLabel="跟随托盘"
            onChange={(next) => set({ taskbarTooltipEntries: next })}
            newWindow="session"
            maxEntries={6}
            minEntries={0}
            addLabel="添加条目"
            disabled={saving || off}
          />
        </V5Section>

        <V5Section
          title="条目组合"
          resetLabel="恢复条带默认"
          resetDisabled={saving || off}
          onReset={() => set({ taskbarWidgetEntries: DEFAULT_ENTRIES })}
          hint="至少 1 条、最多 6 条。条带只画前 4 条，多出来的会标「条带不可见」。"
        >
          <V5EntryList
            entries={entries}
            providers={providers}
            followLabel="跟随托盘"
            onChange={(next) => set({ taskbarWidgetEntries: next })}
            newWindow="weekly"
            maxEntries={6}
            minEntries={1}
            hiddenFrom={4}
            addLabel="添加条目"
            disabled={saving || off}
          />
        </V5Section>

        <V5Section
          title="外观"
          resetLabel="恢复本区外观"
          resetDisabled={saving || off}
          onReset={() => {
            
            
            set({
              taskbarWidgetFontSize: 12,
              taskbarWidgetFontFamily: "Microsoft YaHei UI",
              taskbarWidgetFontWeight: 400,
              taskbarWidgetWidth: 136,
              taskbarWidgetTextAlign: "left",
                taskbarWidgetIconSize: 14,
                taskbarWidgetIconStyle: "pure",
            });
          }}
        >
          <V5Field label="字号" help="10 到 16 像素" off={off}>
            <V5Num
              value={settings.taskbarWidgetFontSize ?? 12}
              min={10}
              max={16}
              unit="px"
              disabled={saving || off}
              onChange={(v) => set({ taskbarWidgetFontSize: v })}
            />
          </V5Field>
          <V5Field
            label="字体"
            help="只列出字重可以连续调的字体。以前存过的普通字体仍能选"
            off={off}
          >
            <select
              className="s5-select"
              value={settings.taskbarWidgetFontFamily}
              disabled={saving || off}
              onChange={(event) =>
                set({ taskbarWidgetFontFamily: event.target.value })
              }
            >
              {(families.length
                ? families
                : [
                    {
                      name: "Microsoft YaHei UI",
                      variableWeight: false,
                      hasCjk: true,
                      recommended: true,
                    },
                  ]
              ).map((family) => (
                <option key={family.name} value={family.name}>
                  {family.name}
                </option>
              ))}
            </select>
          </V5Field>
          <V5Field
            label={`字重（${weightDraft}）`}
            help="100 到 1000，松开鼠标再保存"
            off={off}
          >
            <div>
              <input
                type="range"
                min={100}
                max={1000}
                step={10}
                value={weightDraft}
                disabled={saving || off}
                style={{ width: 180 }}
                onChange={(event) =>
                  setWeightDraft(Number(event.target.value))
                }
                onMouseUp={() =>
                  set({ taskbarWidgetFontWeight: weightDraft })
                }
              />
              <div className="s5-weight-scale">
                <span>细 · 100</span>
                <span>{weightDraft}</span>
                <span>1000 · 粗</span>
              </div>
            </div>
          </V5Field>
          <V5Field
            label="条带宽度"
            help="96 到 240 像素，每格 4。现在程序默认 132，设计默认 136"
            off={off}
          >
            <V5Num
              value={width}
              min={96}
              max={240}
              step={4}
              unit="px"
              disabled={saving || off}
              onChange={(v) => set({ taskbarWidgetWidth: v })}
            />
          </V5Field>
          <V5Field label="文本对齐" off={off}>
            <V5Seg
              value={settings.taskbarWidgetTextAlign}
              disabled={saving || off}
              options={[
                { value: "left", label: "左" },
                { value: "center", label: "中" },
                { value: "right", label: "右" },
              ]}
              onChange={(value) =>
                set({
                  taskbarWidgetTextAlign: value as TaskbarWidgetTextAlign,
                })
              }
            />
          </V5Field>
          <V5Field label="图标尺寸" help="10 到 18 像素，默认 14" off={off}>
            <V5Num
              value={iconSize}
              min={10}
              max={18}
              unit="px"
              disabled={saving || off}
              onChange={(v) => set({ taskbarWidgetIconSize: v })}
            />
          </V5Field>
          <V5Field
            label="图标渲染样式"
            help="三种都要做进产品和预览"
            off={off}
          >
            <V5Seg
              value={iconStyle}
              disabled={saving || off}
              options={[
                { value: "pure", label: "纯彩色图标" },
                { value: "badge", label: "微底色胶囊" },
                { value: "solid", label: "实色徽章" },
              ]}
              onChange={(value) => set({ taskbarWidgetIconStyle: value as HtmlIconStyle })}
            />
          </V5Field>
          <p className="s5-hint">颜色跟系统任务栏走，不能自定义背景。</p>
        </V5Section>
      </div>

      <SurfacePreviewFrame kind="taskbar">
        <TaskbarStripPreview
          enabled={enabled}
          cells={previewLines}
            entries={entries}
            fontSize={settings.taskbarWidgetFontSize ?? 12}
            fontWeight={settings.taskbarWidgetFontWeight ?? 400}
            fontFamily={settings.taskbarWidgetFontFamily}
            textAlign={settings.taskbarWidgetTextAlign ?? "left"}
          width={width}
          iconSize={iconSize}
          iconStyle={iconStyle}
            label="实时预览"
        />
      </SurfacePreviewFrame>
    </div>
  );
}
