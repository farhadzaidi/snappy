import { useEffect, useId, useRef, type ReactNode } from "react";

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

/** Focus starts on Cancel, Escape cancels, and the action looks destructive. */
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
