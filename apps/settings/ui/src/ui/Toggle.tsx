import { useId } from "react";

interface ToggleProps {
  label: string;
  /** A short line under the label, as rows have. */
  hint?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  title?: string;
}

/** A labeled on/off switch, as tall as a row with a control. */
export function Toggle({ label, hint, checked, onChange, disabled = false, title }: ToggleProps) {
  const hintId = useId();
  return (
    <button
      type="button"
      role="switch"
      className="toggle"
      aria-checked={checked}
      aria-label={label}
      aria-describedby={hint === undefined ? undefined : hintId}
      disabled={disabled}
      title={title}
      onClick={() => onChange(!checked)}
    >
      <span className="toggle-text">
        <span className="toggle-label">{label}</span>
        {hint !== undefined && (
          <span id={hintId} className="toggle-hint">
            {hint}
          </span>
        )}
      </span>
      <span className="toggle-track" data-on={checked}>
        <span className="toggle-thumb" />
      </span>
    </button>
  );
}
