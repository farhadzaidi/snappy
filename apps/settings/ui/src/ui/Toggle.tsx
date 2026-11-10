interface ToggleProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  title?: string;
}

/** A labeled on/off switch. */
export function Toggle({ label, checked, onChange, disabled = false, title }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      className="toggle"
      aria-checked={checked}
      disabled={disabled}
      title={title}
      onClick={() => onChange(!checked)}
    >
      <span className="toggle-label">{label}</span>
      <span className="toggle-track" data-on={checked}>
        <span className="toggle-thumb" />
      </span>
    </button>
  );
}
