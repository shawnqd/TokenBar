import { Field } from "../../components/FormControls";

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
 * These were a row of toggle chips. Chips put every option on screen at once,
 * which is fine for three and poor for a dozen: the row wrapped across lines
 * and pushed the settings below it down, and a pressed chip and an unpressed
 * one differ only by a fill, so the current selection had to be read rather
 * than seen. A dropdown states the selection on the closed control and keeps
 * the choosing out of the way until it is wanted.
 *
 * `<details>` rather than a bespoke popup: it opens on click and on Enter,
 * closes on Escape, and is reachable by keyboard, all without a focus trap of
 * our own. A `<select multiple>` was the other candidate and is worse here —
 * it needs ctrl-click to select more than one, which no one guesses.
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

  const commit = (next: string[]) => {
    const ordered = options
      .map((option) => option.value)
      .filter((candidate) => next.includes(candidate));
    onChange(ordered.length === options.length ? [] : ordered);
  };

  const summary =
    value.length === 0
      ? allLabel
      : options
          .filter((option) => selected.includes(option.value))
          .map((option) => option.label)
          .join("、");

  return (
    <Field label={label} description={description}>
      <details className="multi-select" aria-disabled={disabled}>
        <summary className="multi-select__summary">
          <span className="multi-select__value">{summary}</span>
        </summary>
        <div className="multi-select__menu" role="group" aria-label={label}>
          {options.map((option) => {
            const checked = selected.includes(option.value);
            return (
              <label key={option.value} className="multi-select__option">
                <input
                  type="checkbox"
                  checked={checked}
                  // Unchecking the last one would store `[]`, which reads back
                  // as "all" — the control would appear to undo itself.
                  disabled={disabled || (checked && selected.length <= 1)}
                  onChange={() =>
                    commit(
                      checked
                        ? selected.filter((other) => other !== option.value)
                        : [...selected, option.value],
                    )
                  }
                />
                <span>{option.label}</span>
              </label>
            );
          })}
        </div>
      </details>
    </Field>
  );
}
