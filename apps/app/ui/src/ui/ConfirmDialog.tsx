import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";

import { Button } from "./Button";

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmTitle: string;
  cancelTitle?: string;
  icon: ReactNode;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Tab and Shift-Tab wrap around the dialog's buttons, as a modal dialog keeps focus. */
function keepFocusInside(event: KeyboardEvent<HTMLElement>) {
  const buttons = [...event.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled)")];
  const first = buttons[0];
  const last = buttons.at(-1);
  const at = document.activeElement;
  if (event.shiftKey ? at === first || !buttons.includes(at as HTMLElement) : at === last) {
    event.preventDefault();
    (event.shiftKey ? last : first)?.focus();
  }
}

/** Focus starts on Cancel and stays in the dialog, Escape cancels, and the action looks destructive. */
export function ConfirmDialog({
  title,
  message,
  confirmTitle,
  cancelTitle = "Cancel",
  icon,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const cancel = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const messageId = useId();

  useEffect(() => cancel.current?.focus(), []);

  return (
    <div className="dialog-backdrop">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={messageId}
        className="dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            onCancel();
          } else if (event.key === "Tab") {
            keepFocusInside(event);
          }
        }}
      >
        <div className="dialog-icon">{icon}</div>
        <h2 id={titleId} className="dialog-title">
          {title}
        </h2>
        <p id={messageId} className="dialog-message">
          {message}
        </p>
        <div className="dialog-actions">
          <Button ref={cancel} onClick={onCancel}>
            {cancelTitle}
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            {confirmTitle}
          </Button>
        </div>
      </div>
    </div>
  );
}
