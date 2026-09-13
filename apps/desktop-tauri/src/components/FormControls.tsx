import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type React from "react";
import { createPortal } from "react-dom";

// ── tiny reusable controls ──────────────────────────────────────────

export function Toggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
  disabled?: boolean;
}) {
  const input = (
    <input
      type="checkbox"
      className="toggle"
      checked={checked}
      disabled={disabled}
      onChange={(e) => onChange(e.target.checked)}
    />
  );
  if (label) {
    return (
      <label className={`toggle-label ${disabled ? "toggle-label--disabled" : ""}`}>
        {input}
        <span>{label}</span>
      </label>
    );
  }
  return input;
}

export function SegmentedControl({
  value,
  options,
  onChange,
  disabled,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
  disabled?: boolean;
}) {
  const index = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );
  return (
    <div
      className={`segmented${disabled ? " segmented--disabled" : ""}`}
      role="radiogroup"
    >
      <div
        className="segmented__thumb"
        aria-hidden
        style={{
          width: `calc((100% - 4px) / ${options.length})`,
          transform: `translateX(${index * 100}%)`,
        }}
      />
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          disabled={disabled}
          className={`segmented__option${o.value === value ? " is-active" : ""}`}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function ChevronIcon() {
  return (
    <svg
      className="dropdown__chevron"
      width="10"
      height="6"
      viewBox="0 0 10 6"
      fill="none"
      aria-hidden
    >
      <path
        d="M1 1l4 4 4-4"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg
      className="dropdown__check"
      width="12"
      height="10"
      viewBox="0 0 12 10"
      fill="none"
      aria-hidden
    >
      <path
        d="M1 5l3.2 3.2L11 1.2"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

type DropdownRect = {
  top?: number;
  bottom?: number;
  /** Distance from the viewport's right edge to the trigger's right edge. */
  right: number;
  width: number;
  placement: "above" | "below";
  /** Inline max-height only when neither side has enough room. */
  maxHeight?: number;
};

const DROPDOWN_GAP = 4;
const DROPDOWN_VIEWPORT_MARGIN = 8;

/** Shared open/close plumbing for the themed dropdown trigger + portaled panel. */
function useDropdownPanel() {
  const [open, setOpen] = useState(false);
  const [rect, setRect] = useState<DropdownRect | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const close = useCallback(() => setOpen(false), []);

  const openPanel = useCallback(() => {
    const r = triggerRef.current?.getBoundingClientRect();
    if (r) {
      // Pin the panel to the trigger's right edge via `right` (not left +
      // translateX). Settings controls sit on the row's right, and an inline
      // transform would fight the open animation's own transform keyframes.
      setRect({
        top: r.bottom + DROPDOWN_GAP,
        right: window.innerWidth - r.right,
        width: r.width,
        placement: "below",
      });
    }
    setOpen(true);
  }, []);

  // The panel is portaled, so its height is not known until after the first
  // render. Measure it before paint and flip it above the trigger when the
  // lower viewport does not have enough room. If neither side fits, constrain
  // the panel to the side with more space so its own list can scroll instead
  // of escaping the settings window.
  const positionPanel = useCallback(() => {
    const trigger = triggerRef.current?.getBoundingClientRect();
    const panel = panelRef.current;
    if (!trigger || !panel) return;

    const viewportHeight = window.innerHeight;
    // `offsetHeight` is not affected by the opening scale animation; fall
    // back to the rect for test environments that do not implement layout.
    const panelHeight = panel.offsetHeight || panel.getBoundingClientRect().height;
    const belowSpace = Math.max(
      0,
      viewportHeight - trigger.bottom - DROPDOWN_GAP - DROPDOWN_VIEWPORT_MARGIN,
    );
    const aboveSpace = Math.max(
      0,
      trigger.top - DROPDOWN_GAP - DROPDOWN_VIEWPORT_MARGIN,
    );
    const placeAbove = panelHeight > belowSpace && aboveSpace > belowSpace;
    const availableSpace = placeAbove ? aboveSpace : belowSpace;
    const maxHeight =
      panelHeight > availableSpace && availableSpace > 0
        ? Math.floor(availableSpace)
        : undefined;

    setRect((previous) => {
      if (!previous) return previous;
      return {
        ...previous,
        placement: placeAbove ? "above" : "below",
        top: placeAbove ? undefined : trigger.bottom + DROPDOWN_GAP,
        bottom: placeAbove
          ? viewportHeight - trigger.top + DROPDOWN_GAP
          : undefined,
        maxHeight,
      };
    });
  }, []);

  useLayoutEffect(() => {
    if (open) positionPanel();
  }, [open, positionPanel]);

  useEffect(() => {
    if (!open) return undefined;

    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (triggerRef.current?.contains(target) || panelRef.current?.contains(target)) {
        return;
      }
      close();
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    // Close when the *page* scrolls under the portaled panel, but not when the
    // panel itself scrolls — font lists are long and `overflow-y: auto` on the
    // panel would otherwise fire this capture listener and dismiss the menu
    // on the first wheel tick.
    const handleScroll = (e: Event) => {
      const target = e.target;
      if (
        target instanceof Node &&
        panelRef.current &&
        (target === panelRef.current || panelRef.current.contains(target))
      ) {
        return;
      }
      close();
    };
    document.addEventListener("pointerdown", handlePointerDown, true);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("scroll", handleScroll, true);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("scroll", handleScroll, true);
      window.removeEventListener("resize", close);
    };
  }, [open, close]);

  return { open, close, openPanel, rect, triggerRef, panelRef };
}

/**
 * Custom-styled dropdown replacing the native `<select>` — the OS-native
 * popup can't be themed (always renders with the platform's default white
 * listbox on Windows/WebView2), which breaks the app's dark UI. Renders
 * its option list in a portal so it can float above scrollable containers
 * (e.g. `.settings-body`) without being clipped.
 */
export function Select({
  value,
  options,
  onChange,
  disabled,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
  disabled?: boolean;
}) {
  const { open, close, openPanel, rect, triggerRef, panelRef } = useDropdownPanel();

  const selectedLabel = options.find((option) => option.value === value)?.label ?? value;

  return (
    <div className="dropdown">
      <button
        type="button"
        ref={triggerRef}
        className={`dropdown__trigger${open ? " dropdown__trigger--open" : ""}`}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? close() : openPanel())}
      >
        <span className="dropdown__value">{selectedLabel}</span>
        <ChevronIcon />
      </button>
      {open &&
        rect &&
        createPortal(
          <div
            ref={panelRef}
            role="listbox"
            className={`dropdown__panel${
              rect.placement === "above" ? " dropdown__panel--above" : ""
            }`}
            style={{
              top: rect.top,
              bottom: rect.bottom,
              right: rect.right,
              minWidth: rect.width,
              maxHeight: rect.maxHeight,
            }}
          >
            {options.map((o) => (
              <button
                key={o.value}
                type="button"
                role="option"
                aria-selected={o.value === value}
                className={`dropdown__option${o.value === value ? " is-selected" : ""}`}
                onClick={() => {
                  onChange(o.value);
                  close();
                }}
              >
                <span className="dropdown__option-label">{o.label}</span>
                {o.value === value && <CheckIcon />}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}

/**
 * Multi-select sibling of `Select`. Same trigger, panel, option rows and check
 * marks — only the commit semantics differ: options toggle without closing, and
 * the last selected option cannot be cleared (callers that treat empty as "all"
 * would otherwise see the control undo itself).
 */
export function MultiSelect({
  values,
  options,
  onChange,
  disabled,
  summary,
  "aria-label": ariaLabel,
}: {
  /** Currently selected values (already expanded — empty is not special here). */
  values: string[];
  options: { value: string; label: string }[];
  onChange: (next: string[]) => void;
  disabled?: boolean;
  /** Closed-control label (e.g. "全部" or a joined list of names). */
  summary: string;
  "aria-label"?: string;
}) {
  const { open, close, openPanel, rect, triggerRef, panelRef } = useDropdownPanel();

  const selected = new Set(values);

  const toggle = (candidate: string) => {
    const checked = selected.has(candidate);
    if (checked && selected.size <= 1) return;
    const next = checked
      ? values.filter((other) => other !== candidate)
      : [...values, candidate];
    // Preserve the option order so the stored roster matches the control order.
    const ordered = options
      .map((option) => option.value)
      .filter((value) => next.includes(value));
    onChange(ordered);
  };

  return (
    <div className="dropdown">
      <button
        type="button"
        ref={triggerRef}
        className={`dropdown__trigger${open ? " dropdown__trigger--open" : ""}`}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        onClick={() => (open ? close() : openPanel())}
      >
        <span className="dropdown__value">{summary}</span>
        <ChevronIcon />
      </button>
      {open &&
        rect &&
        createPortal(
          <div
            ref={panelRef}
            role="listbox"
            aria-multiselectable
            aria-label={ariaLabel}
            className={`dropdown__panel${
              rect.placement === "above" ? " dropdown__panel--above" : ""
            }`}
            style={{
              top: rect.top,
              bottom: rect.bottom,
              right: rect.right,
              minWidth: Math.max(rect.width, 160),
              maxHeight: rect.maxHeight,
            }}
          >
            {options.map((o) => {
              const isSelected = selected.has(o.value);
              const lastSelected = isSelected && selected.size <= 1;
              return (
                <button
                  key={o.value}
                  type="button"
                  role="option"
                  aria-selected={isSelected}
                  disabled={disabled || lastSelected}
                  className={`dropdown__option${isSelected ? " is-selected" : ""}`}
                  onClick={() => toggle(o.value)}
                >
                  <span className="dropdown__option-label">{o.label}</span>
                  {isSelected && <CheckIcon />}
                </button>
              );
            })}
          </div>,
          document.body,
        )}
    </div>
  );
}

/**
 * Numeric field as `−  [value]  +`. Used for size, width, thresholds, volume —
 * everywhere a bounded number is edited. The middle field stays free-type so
 * multi-digit entry is not forced through single steps; blur/Enter clamps to
 * min/max. Arrow keys and the side buttons nudge by `step`.
 */
export function NumberInput({
  value,
  min,
  max,
  step = 1,
  onChange,
  disabled,
}: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (v: number) => void;
  disabled?: boolean;
}) {
  const clamp = useCallback(
    (n: number) => {
      let next = n;
      if (min !== undefined) next = Math.max(min, next);
      if (max !== undefined) next = Math.min(max, next);
      return next;
    },
    [min, max],
  );

  // Draft is a string so partial typing ("1", "") is not rounded away mid-edit.
  const [draft, setDraft] = useState(() => String(value));
  useEffect(() => {
    setDraft(String(value));
  }, [value]);

  const commit = useCallback(
    (raw: string) => {
      const parsed = Number(raw);
      if (!Number.isFinite(parsed)) {
        setDraft(String(value));
        return;
      }
      const next = clamp(parsed);
      setDraft(String(next));
      if (next !== value) onChange(next);
    },
    [clamp, onChange, value],
  );

  const nudge = useCallback(
    (direction: -1 | 1) => {
      const next = clamp(value + direction * step);
      if (next !== value) onChange(next);
    },
    [clamp, onChange, step, value],
  );

  const atMin = min !== undefined && value <= min;
  const atMax = max !== undefined && value >= max;

  return (
    <div
      className={`number-stepper${disabled ? " number-stepper--disabled" : ""}`}
    >
      <button
        type="button"
        className="number-stepper__btn"
        disabled={disabled || atMin}
        aria-label="−"
        tabIndex={-1}
        onClick={() => nudge(-1)}
      >
        −
      </button>
      <input
        type="text"
        inputMode="decimal"
        className="number-stepper__input"
        role="spinbutton"
        aria-valuenow={value}
        aria-valuemin={min}
        aria-valuemax={max}
        value={draft}
        disabled={disabled}
        onChange={(e) => {
          const raw = e.target.value;
          setDraft(raw);
          // Commit as soon as the field is a finite number so free-typing
          // behaves like the old <input type="number"> (and tests that fire
          // change without blur still see the write). Empty / partial strings
          // wait for blur to snap back.
          if (raw.trim() === "" || raw === "-" || raw === "." || raw === "-.") {
            return;
          }
          const parsed = Number(raw);
          if (Number.isFinite(parsed)) {
            const next = clamp(parsed);
            if (next !== value) onChange(next);
          }
        }}
        onBlur={() => commit(draft)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit(draft);
            (e.target as HTMLInputElement).blur();
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            nudge(1);
          } else if (e.key === "ArrowDown") {
            e.preventDefault();
            nudge(-1);
          }
        }}
      />
      <button
        type="button"
        className="number-stepper__btn"
        disabled={disabled || atMax}
        aria-label="+"
        tabIndex={-1}
        onClick={() => nudge(1)}
      >
        +
      </button>
    </div>
  );
}

export function TextInput({
  value,
  placeholder,
  onChange,
  disabled,
}: {
  value: string;
  placeholder?: string;
  onChange: (v: string) => void;
  disabled?: boolean;
}) {
  return (
    <input
      type="text"
      className="text-input"
      value={value}
      placeholder={placeholder}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

// ── field row ────────────────────────────────────────────────────────

export function Field({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="settings-field">
      <div className="settings-field__text">
        <span className="settings-field__label">{label}</span>
        {description && (
          <span className="settings-field__desc">{description}</span>
        )}
      </div>
      <div className="settings-field__control">{children}</div>
    </div>
  );
}
