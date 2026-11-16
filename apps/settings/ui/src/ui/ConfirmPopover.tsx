import type { ReactNode, RefObject } from "react";

import { Button } from "./Button";
import { Popover } from "./Popover";

interface ConfirmPopoverProps {
  /** Names the popover for assistive technology. */
  label: string;
  question: ReactNode;
  /** The action it asks about, such as "Reset" or "Delete". */
  confirmTitle: string;
  /** The element the popover belongs to; presses inside it don't close the popover. */
  anchor: RefObject<HTMLElement | null>;
  /** The control that asked, which focus goes back to however the popover closes. */
  trigger: RefObject<HTMLElement | null>;
  placement?: "below" | "above";
  onConfirm: () => void;
  onClose: () => void;
}

/**
 * Asks before a destructive action, at the end of its anchor. Focus starts on Cancel, and
 * returns to the control that asked once the popover closes, so the keyboard picks up where
 * it left off.
 */
export function ConfirmPopover({
  label,
  question,
  confirmTitle,
  anchor,
  trigger,
  placement = "below",
  onConfirm,
  onClose,
}: ConfirmPopoverProps) {
  const close = () => {
    onClose();
    trigger.current?.focus();
  };

  return (
    <Popover
      label={label}
      anchor={anchor}
      align="end"
      placement={placement}
      onClose={close}
      className="confirm-popover"
    >
      <p className="confirm-title">{question}</p>
      <div className="confirm-actions">
        <Button autoFocus onClick={close}>
          Cancel
        </Button>
        <Button
          variant="destructive"
          onClick={() => {
            close();
            onConfirm();
          }}
        >
          {confirmTitle}
        </Button>
      </div>
    </Popover>
  );
}
