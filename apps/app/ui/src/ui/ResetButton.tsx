import { useRef, useState, type ReactNode } from "react";

import { ConfirmPopover } from "./ConfirmPopover";
import { RefreshIcon } from "./icons";

/**
 * "Reset" for a panel, which asks `question` first and resets on the answer. It is off while
 * there is nothing to reset.
 */
export function ResetButton({
  label,
  question,
  disabled,
  onConfirm,
}: {
  /** Names what it resets for assistive technology and the tooltip. */
  label: string;
  question: ReactNode;
  disabled: boolean;
  onConfirm: () => void;
}) {
  const [asking, setAsking] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  return (
    <div className="popover-anchor" ref={anchor}>
      <button
        ref={button}
        type="button"
        className="reset-button"
        aria-label={label}
        title={disabled ? "Already the defaults" : label}
        aria-expanded={asking}
        disabled={disabled}
        onClick={() => setAsking(!asking)}
      >
        <RefreshIcon size={13} />
        Reset
      </button>
      {asking && (
        <ConfirmPopover
          label={label}
          question={question}
          confirmTitle="Reset"
          anchor={anchor}
          trigger={button}
          onConfirm={onConfirm}
          onClose={() => setAsking(false)}
        />
      )}
    </div>
  );
}
