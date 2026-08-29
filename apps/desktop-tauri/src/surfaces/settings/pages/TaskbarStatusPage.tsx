import { useEffect, useMemo, useState, useSyncExternalStore, useCallback } from "react";
import { useFontPicker } from "../../../hooks/useFontPicker";
import { useLocale } from "../../../hooks/useLocale";
import type {
  TaskbarEntry,
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
import { V5Field, V5Num, V5Section, V5Seg, V5Select, V5Toggle } from "./v5Controls";
import FontInstallDialog from "../FontInstallDialog";
import type { ProviderSnapshot } from "../../../core/snapshot";
import { projectSurface } from "../../../core/projection";
import type { UsageStore } from "../../../core/usageStore";

const DEFAULT_ENTRIES: TaskbarEntry[] = [
  { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
  { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" },
];

const EMPTY_STORE_STATE_TASKBAR_PAGE: { version: number; records: Record<string, unknown> } = {
  version: 0,
  records: {},
};
function useCoreSnapshotListForTaskbar(store?: UsageStore | null): ProviderSnapshot[] {
  const subscribe = useCallback(
    (cb: () => void) => (store ? store.subscribe(cb) : () => {}),
    [store],
  );
  const getSnapshot = useCallback(
    () => (store ? store.getSnapshot() : EMPTY_STORE_STATE_TASKBAR_PAGE),
    [store],
  );
  const getServerSnapshot = useCallback(() => getSnapshot(), [getSnapshot]);
  const state = useSyncExternalStore(
    subscribe as unknown as Parameters<typeof useSyncExternalStore>[0],
    getSnapshot as never,
    getServerSnapshot as never,
  ) as unknown as { records: Record<string, { snapshot: ProviderSnapshot | null }> };
  return useMemo(
    () => Object.values(state.records).map((r) => r.snapshot).filter(Boolean) as ProviderSnapshot[],
    [state],
  );
}

function deriveTaskbarCellsFromSnapshots(
  snapshots: ProviderSnapshot[],
  entries: TaskbarEntry[],
  showAsUsed: boolean,
): TaskbarStripCell[] {
  if (snapshots.length === 0 || entries.length === 0) return [];
  const byId = new Map(snapshots.map((s) => [s.providerId, s]));
  const fallback = snapshots[0];
  const cells: TaskbarStripCell[] = [];
  for (const entry of entries.slice(0, 4)) {
    const pid = entry.providerId === TASKBAR_PROVIDER_AUTO ? fallback.providerId : entry.providerId;
    const snap = byId.get(pid) ?? fallback;
    const proj = projectSurface(snap, {
      showAsUsed,
      taskbarEntries: [entry],
    });
    // projection.taskbarCells is ordered by entries; single entry → single cell
    const cell = proj.taskbarCells[0] as unknown as TaskbarStripCell | undefined;
    if (cell) {
      const legacy = cell as unknown as Record<string, unknown>;
      if (legacy.glyph === undefined) legacy.glyph = (cell as unknown as { icon?: { fallbackGlyph: string | null } }).icon?.fallbackGlyph ?? null;
      if (legacy.color === undefined) legacy.color = (cell as unknown as { icon?: { brandColor: string | null } }).icon?.brandColor ?? null;
      if (legacy.text === undefined) legacy.text = `${(cell as unknown as { tag: string }).tag} ${(cell as unknown as { value: string }).value}`.trim();
      cells.push(cell as unknown as TaskbarStripCell);
    }
  }
  return cells;
}

export default function TaskbarStatusPage({
  settings,
  set,
  saving,
  coreStore: injectedCoreStore,
}: SettingsPageProps & { coreStore?: UsageStore | null }) {
  const { t } = useLocale();
  const enabled = settings.taskbarWidgetEnabled;
  const off = !enabled;
  const iconSize = settings.taskbarWidgetIconSize ?? 14;
  const iconStyle = (settings.taskbarWidgetIconStyle ?? "pure") as HtmlIconStyle;
  const [weightDraft, setWeightDraft] = useState(
    settings.taskbarWidgetFontWeight ?? 400,
  );
  const entries = settings.taskbarWidgetEntries ?? DEFAULT_ENTRIES;
  const width = settings.taskbarWidgetWidth ?? 136;
  const currentFamily = settings.taskbarWidgetFontFamily || "";
  const {
    options: fontOptions,
    isContinuous: selectedIsVariable,
    defaultFamily,
    install,
    installMiss,
    chooseFamily,
    cancelInstall,
    openInstallPage,
    confirmInstalled,
  } = useFontPicker(currentFamily);

  const hasCoreInjection = injectedCoreStore !== undefined;
  const coreSnapshots = useCoreSnapshotListForTaskbar(injectedCoreStore);
  const coreCells = useMemo(
    () => (hasCoreInjection ? deriveTaskbarCellsFromSnapshots(coreSnapshots, entries, settings.taskbarShowAsUsed ?? true) : []),
    [hasCoreInjection, coreSnapshots, entries, settings.taskbarShowAsUsed],
  );
  // A preview is valid only when it is derived from the injected process
  // projection. The old no-store native preview made Settings a second
  // production read path and could display data from a different refresh.
  const previewLines = hasCoreInjection ? coreCells : [];

  useEffect(() => {
    setWeightDraft(settings.taskbarWidgetFontWeight ?? 400);
  }, [settings.taskbarWidgetFontWeight]);

  const providers = useMemo(
    () => catalogChoices(settings.enabledProviders),
    [settings.enabledProviders],
  );

  const tooltip = settings.taskbarTooltipEntries ?? [];
  
  return (
    <div className="s5-surf-split s5-surf-split--taskbar">
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
              taskbarWidgetFontFamily: defaultFamily,
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
            help="只内置 MiSans VF。其余四种未安装时会引导到官方下载页，装好后点「我已安装」。以前保存的其他字体仍可选"
            off={off}
          >
            <V5Select
              value={currentFamily}
              disabled={saving || off || fontOptions.length === 0}
              options={fontOptions}
              onChange={(value) =>
                chooseFamily(value, (family) => set({ taskbarWidgetFontFamily: family }))
              }
            />
            {install ? (
              <FontInstallDialog
                guide={install}
                miss={installMiss}
                onCancel={cancelInstall}
                onOpenDownload={openInstallPage}
                onRecheck={() =>
                  void confirmInstalled((family) =>
                    set({ taskbarWidgetFontFamily: family }),
                  )
                }
              />
            ) : null}
          </V5Field>
          <V5Field
            label={`字重（${weightDraft}）`}
            help={
              selectedIsVariable
                ? t("TaskbarWidgetFontWeightHelperVariable")
                : t("TaskbarWidgetFontWeightHelperStatic")
            }
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
          <V5Field label="图标间距" help="图标和文字之间的间距，0 到 12 像素，默认 5" off={off}>
            <V5Num
              value={settings.taskbarWidgetIconGapPx ?? 5}
              min={0}
              max={12}
              unit="px"
              disabled={saving || off}
              onChange={(v) => set({ taskbarWidgetIconGapPx: v })}
            />
          </V5Field>
          <V5Field label="数值间距" help="文字和额度数字之间的间距，0 到 8 像素，默认 2" off={off}>
            <V5Num
              value={settings.taskbarWidgetValueGapPx ?? 2}
              min={0}
              max={8}
              unit="px"
              disabled={saving || off}
              onChange={(v) => set({ taskbarWidgetValueGapPx: v })}
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
