import type { CSSProperties } from "react";
import { ProviderIcon } from "../../../components/providers/ProviderIcon";
import { getProviderIcon } from "../../../components/providers/providerIcons";
import type {
  TaskbarEntry,
  TaskbarPreviewLine,
  TaskbarStripCell,
  TaskbarWidgetTextAlign,
} from "../../../types/bridge";
import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";

export type TaskbarIconStyle = "pure" | "badge" | "solid";

/**
 * Marks with a landscape / wide wordmark are shrunk to 72% inside their square
 * slot so they sit visually lighter than Grok's official badge rather than
 * stretching the cell (design/taskbar-strip-style-options.html).
 */
const WIDE_MARKS = new Set(["deepseek", "kimi", "kimik2", "minimax", "mistral"]);

/** Grok ships its own black badge + white glyph; it must never get a tile backing. */
const BADGE_SELF_MARKS = new Set(["grok"]);

interface Props {
  /** New native cell format from `get_taskbar_preview_lines`. */
  cells?: TaskbarStripCell[];
  /** Legacy `TaskbarPreviewLine[]` for older callers; normalized when no cells given. */
  lines?: TaskbarPreviewLine[];
  entries: TaskbarEntry[];
  enabled: boolean;
  width: number;
  fontSize: number;
  fontWeight: number;
  fontFamily: string;
  textAlign: TaskbarWidgetTextAlign;
  iconSize: number;
  iconStyle: TaskbarIconStyle;
  label: string;
}

export function splitPreviewText(text: string): { tag: string; value: string } {
  const trimmed = text.trim();
  const match = trimmed.match(
    /^(.*?)(?:\s+)([¥$€£]\s*[\d.,]+|\d+(?:\.\d+)?%|\d+(?:\.\d+)?)$/u,
  );
  if (match) {
    return { tag: match[1].trim(), value: match[2] };
  }
  if (/^[¥$€£]/.test(trimmed)) {
    return { tag: "", value: trimmed };
  }
  return { tag: "", value: trimmed };
}

const COLOR_HINT_IDS = [
  "claude",
  "codex",
  "gemini",
  "deepseek",
  "grok",
  "kimi",
  "minimax",
  "mistral",
  "cursor",
  "openaiapi",
];

function resolveProviderId(
  line: TaskbarPreviewLine,
  entry: TaskbarEntry | undefined,
): string | null {
  if (entry?.providerId && entry.providerId !== TASKBAR_PROVIDER_AUTO) {
    return entry.providerId;
  }
  if (line.color) {
    const wanted = line.color.toLowerCase();
    for (const id of COLOR_HINT_IDS) {
      if (getProviderIcon(id).brandColor.toLowerCase() === wanted) {
        return id;
      }
    }
  }
  return null;
}

/**
 * Turn a legacy `TaskbarPreviewLine` (glyph/color/text) into the cell shape the
 * preview renders, resolving the provider id from the entry then the brand
 * colour hint so old streams keep painting the official mark.
 */
export function normalizeLegacyLine(
  line: TaskbarPreviewLine,
  entry: TaskbarEntry | undefined,
): TaskbarStripCell {
  const { tag, value } = splitPreviewText(line.text);
  const providerId = resolveProviderId(line, entry);
  const brand = providerId ? getProviderIcon(providerId).brandColor : line.color;
  return {
    providerId: providerId ?? "",
    window: "primary",
    tag,
    value: value || line.text,
    state: "ready",
    reason: null,
    icon:
      providerId && brand
        ? { providerId, assetId: providerId, brandColor: brand, fallbackGlyph: line.glyph ?? null }
        : null,
    glyph: line.glyph,
    color: line.color,
    text: line.text,
  };
}

/**
 * The settings preview for the native taskbar strip.
 *
 * Renders the cells `get_taskbar_preview_lines` returned verbatim (never a
 * fabricated imitation): a brand mark + short tag + value kept as one
 * left-packed cluster, exactly as the strip does. Lifecycle state is respected —
 * error / unsupported / notConfigured cells show the reason (or an em dash),
 * never a made-up percentage.
 */
export default function TaskbarStripPreview({
  cells,
  lines,
  entries,
  enabled,
  width,
  fontSize,
  fontWeight,
  fontFamily,
  textAlign,
  iconSize,
  iconStyle,
}: Props) {
  const normalized = cells ?? (lines ?? []).map((line, idx) =>
    normalizeLegacyLine(line, entries[idx]),
  );
  const visible = normalized.slice(0, 4);
  const count = visible.length;
  const columns = count === 0 ? 1 : Math.max(1, Math.ceil(count / 2));
  const rows = count === 0 ? 1 : Math.max(1, Math.ceil(count / columns));
  const clusterAlign =
    textAlign === "center" ? "center" : textAlign === "right" ? "flex-end" : "flex-start";

  return (
    <div className="settings-taskbar-preview taskbar-preview">
      <div
        className={`taskbar-preview__strip${enabled ? "" : " is-disabled"}`}
        data-icon-style={iconStyle}
        style={{
          width: `${Math.max(96, Math.min(240, width))}px`,
          maxWidth: "100%",
          fontSize: `${fontSize}px`,
          fontWeight,
          fontVariationSettings: `"wght" ${fontWeight}`,
          fontFamily,
          gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
          gridTemplateRows: `repeat(${rows}, 1fr)`,
        }}
      >
        {visible.map((cell, index) => {
          const column = Math.floor(index / rows) + 1;
          const row = (index % rows) + 1;
          const providerId = cell.icon ? cell.icon.providerId : cell.providerId || null;
          const brand =
            cell.icon?.brandColor ??
            (providerId ? getProviderIcon(providerId).brandColor : null) ??
            cell.color ??
            null;
          const wide = providerId ? WIDE_MARKS.has(providerId) : false;
          const selfBadge = providerId ? BADGE_SELF_MARKS.has(providerId) : false;
          const isProblem =
            cell.state === "error" ||
            cell.state === "unsupported" ||
            cell.state === "notConfigured";
          const shownValue = isProblem
            ? (cell.reason || cell.value || "—")
            : (cell.value || "—");
          const tag = cell.tag?.trim() ?? "";
          const val = shownValue || "—";
          return (
            <span
              key={`${cell.text}-${index}`}
              className="taskbar-preview__line"
              style={{
                gridColumn: column,
                gridRow: row,
                justifyContent: clusterAlign,
              }}
            >
              <span className="taskbar-preview__cluster">
                <span
                  className={`taskbar-preview__mark${wide ? " is-wide" : ""}${selfBadge ? " is-self-badge" : ""}`}
                  style={
                    {
                      color: brand ?? undefined,
                      width: iconSize,
                      height: iconSize,
                      maxWidth: iconSize,
                    } as CSSProperties
                  }
                >
                  {providerId ? (
                    <ProviderIcon providerId={providerId} size={iconSize} />
                  ) : (
                    (cell.icon?.fallbackGlyph ?? cell.glyph)
                  )}
                </span>
                {tag ? <span className="taskbar-preview__tag">{tag} </span> : null}
                <span className="taskbar-preview__val">{val}</span>
              </span>
            </span>
          );
        })}
      </div>
    </div>
  );
}
