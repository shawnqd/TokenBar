import { Field, SegmentedControl } from "../../components/FormControls";

interface Props {
  label: string;
  description?: string;
  /** Text for the `true` state. */
  onLabel: string;
  /** Text for the `false` state. */
  offLabel: string;
  value: boolean;
  disabled?: boolean;
  onChange: (value: boolean) => void;
}

/**
 * A setting that is stored as a boolean but reads as a choice between two named
 * things — used quota vs remaining, a countdown vs the moment itself, and so on.
 *
 * These were switches, and a switch is the wrong control for them: it shows one
 * label and leaves the off state to be inferred. "显示为已用: 关" does not tell
 * you the alternative is 剩余; you have to toggle it and look at the result.
 * Both states are named here, and the selected one is visibly selected.
 *
 * Genuine on/off settings — start at login, notifications, sound — keep their
 * switches. Turning those into two pills reading 开启 / 关闭 would be longer to
 * scan and no clearer, which is the opposite of the point.
 */
export default function BinaryChoiceField({
  label,
  description,
  onLabel,
  offLabel,
  value,
  disabled,
  onChange,
}: Props) {
  return (
    <Field label={label} description={description}>
      <SegmentedControl
        value={value ? "on" : "off"}
        disabled={disabled}
        options={[
          { value: "on", label: onLabel },
          { value: "off", label: offLabel },
        ]}
        onChange={(next) => onChange(next === "on")}
      />
    </Field>
  );
}
