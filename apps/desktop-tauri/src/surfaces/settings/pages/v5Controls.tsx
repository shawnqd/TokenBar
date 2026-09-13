import type { CSSProperties, ReactNode } from "react";
import { Select } from "../../../components/FormControls";

export function V5Section({
  title,
  resetLabel,
  onReset,
  resetDisabled,
  hint,
  children,
}: {
  title: string;
  resetLabel?: string;
  onReset?: () => void;
  resetDisabled?: boolean;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <section className="s5-section">
      <div className="s5-section-h">
        <h3>{title}</h3>
        {onReset && resetLabel ? (
          <button
            type="button"
            className="s5-reset"
            disabled={resetDisabled}
            onClick={onReset}
          >
            {resetLabel}
          </button>
        ) : null}
      </div>
      {hint ? <p className="s5-hint">{hint}</p> : null}
      {children}
    </section>
  );
}

export function V5Field({
  label,
  help,
  off,
  children,
}: {
  label: string;
  help?: string;
  off?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={`s5-field${off ? " is-off" : ""}`}>
      <div className="s5-field-copy">
        <div className="s5-field-label">{label}</div>
        {help ? (
          <div className="s5-field-help" title={help}>
            {help}
          </div>
        ) : null}
      </div>
      {children}
    </div>
  );
}

export function V5Toggle({
  on,
  onChange,
  disabled,
  label,
}: {
  on: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <button
      type="button"
      className={`s5-toggle${on ? " on" : ""}`}
      aria-pressed={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
    />
  );
}

export function V5Seg({
  value,
  options,
  onChange,
  disabled,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const selectedIndex = Math.max(
    0,
    options.findIndex((option) => option.value === value),
  );
  const segStyle = {
    "--s5-seg-count": options.length,
    "--s5-seg-index": selectedIndex,
  } as CSSProperties;

  return (
    <div className="s5-seg" role="radiogroup" style={segStyle}>
      <span className="s5-seg__indicator" aria-hidden="true" />
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          className={option.value === value ? "on" : undefined}
          disabled={disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function V5Select({
  value,
  options,
  onChange,
  disabled,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="s5-dropdown">
      <Select
        value={value}
        options={options}
        onChange={onChange}
        disabled={disabled}
      />
    </div>
  );
}

export function V5Num({
  value,
  min,
  max,
  step,
  unit,
  disabled,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  disabled?: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <div className="s5-range-row">
      <input
        className="s5-num"
        type="number"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={(event) => {
          const next = Number(event.target.value);
          if (Number.isFinite(next)) onChange(next);
        }}
      />
      {unit ? <span className="s5-unit">{unit}</span> : null}
    </div>
  );
}

export function ConfirmDialog({
  text,
  onCancel,
  onConfirm,
}: {
  text: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="s5-modal-back" data-settings-dialog>
      <div className="s5-modal" role="dialog" aria-modal="true">
        <h4>确认</h4>
        <p>{text}</p>
        <div className="s5-modal-actions">
          <button type="button" className="s5-ghost" onClick={onCancel}>
            取消
          </button>
          <button type="button" className="s5-ghost danger" onClick={onConfirm}>
            确认
          </button>
        </div>
      </div>
    </div>
  );
}
