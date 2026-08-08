import { Field } from "../../components/FormControls";

interface Props {
  label: string;
  description: string;
  /** Every provider the user has switched on, in settings order. */
  enabledProviderIds: string[];
  /** This component's filter. Empty means "follow the enabled list". */
  value: string[];
  disabled?: boolean;
  onChange: (next: string[]) => void;
}

/**
 * Which providers one surface shows — the floating bar's `floatBarProviderIds`
 * and the dashboard's `dashboardProviderIds` (task package item H: each
 * component picks its own providers rather than all three inheriting the global
 * enabled list).
 *
 * Every chip is pressed when the filter is empty, because empty *means* all.
 * The two are stored differently on purpose: an empty list keeps following the
 * enabled list, so a provider switched on later appears here without the user
 * having to come back and tick it. Selecting every chip therefore writes an
 * empty list rather than an explicit roster that would freeze the surface at
 * today's providers.
 *
 * The last pressed chip cannot be unpressed. An empty roster would render as
 * "all" on the next load, so the control would visibly undo itself; a surface
 * showing nothing is not a state worth being able to reach by accident.
 */
export default function ProviderFilterField({
  label,
  description,
  enabledProviderIds,
  value,
  disabled,
  onChange,
}: Props) {
  const selected =
    value.length === 0
      ? enabledProviderIds
      : enabledProviderIds.filter((id) => value.includes(id));

  const commit = (next: string[]) => {
    // Ordered by the enabled list so the stored value does not depend on the
    // order the user happened to click in.
    const ordered = enabledProviderIds.filter((id) => next.includes(id));
    onChange(ordered.length === enabledProviderIds.length ? [] : ordered);
  };

  return (
    <Field label={label} description={description}>
      <div className="option-chips" role="group">
        {enabledProviderIds.map((id) => {
          const active = selected.includes(id);
          const isLastActive = active && selected.length <= 1;
          return (
            <button
              key={id}
              type="button"
              className="option-chips__chip"
              aria-pressed={active}
              disabled={disabled || isLastActive}
              onClick={() =>
                commit(
                  active
                    ? selected.filter((other) => other !== id)
                    : [...selected, id],
                )
              }
            >
              {id}
            </button>
          );
        })}
      </div>
    </Field>
  );
}
