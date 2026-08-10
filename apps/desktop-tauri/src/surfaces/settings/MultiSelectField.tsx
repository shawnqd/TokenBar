import { Field, MultiSelect } from "../../components/FormControls";

interface Option {
  value: string;
  label: string;
}

interface Props {
  label: string;
  description: string;
  options: Option[];
  /** Selected values. Empty means "all", and is stored that way — see below. */
  value: string[];
  disabled?: boolean;
  onChange: (next: string[]) => void;
  /** Shown on the closed control when nothing is narrowed. */
  allLabel: string;
}

/**
 * A dropdown for choosing several of a small set — which providers a surface
 * shows, which quota windows its cards carry.
 *
 * These were a row of toggle chips, then a bespoke `<details>` menu. Both
 * looked different from the rest of Settings. The control is now the shared
 * `MultiSelect` (same trigger/panel/check marks as `Select`); only the
 * empty-means-all encoding lives here.
 *
 * **Empty means all, and is stored as empty.** Selecting every option writes
 * `[]`, so a provider added later is included without the user coming back to
 * tick it; an explicit roster would silently freeze the surface at today's set.
 * The last selected option cannot be cleared — an empty roster would reappear
 * as "all" on the next load, so the control would visibly undo itself.
 */
export default function MultiSelectField({
  label,
  description,
  options,
  value,
  disabled,
  onChange,
  allLabel,
}: Props) {
  const selected =
    value.length === 0
      ? options.map((option) => option.value)
      : options.filter((option) => value.includes(option.value)).map((o) => o.value);

  const summary =
    value.length === 0
      ? allLabel
      : options
          .filter((option) => selected.includes(option.value))
          .map((option) => option.label)
          .join("、");

  return (
    <Field label={label} description={description}>
      <MultiSelect
        values={selected}
        options={options}
        summary={summary}
        disabled={disabled}
        aria-label={label}
        onChange={(next) =>
          onChange(next.length === options.length ? [] : next)
        }
      />
    </Field>
  );
}
