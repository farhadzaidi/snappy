import { useId, useRef, useState } from "react";

import { useServices } from "../../state/services";

interface PixelFieldProps {
  title: string;
  value: number;
  /** Apply a typed value; the field then shows the value that actually resulted. */
  onCommit: (value: number) => void;
}

function shown(value: number): string {
  return String(Math.round(value));
}

/**
 * Commits on Enter, on leaving the field, or on Save; Escape cancels typing.
 * While it holds a different number, Save counts that as a change.
 */
export function PixelField({ title, value, onCommit }: PixelFieldProps) {
  const { editor } = useServices();
  const [typed, setTyped] = useState<string | null>(null);
  const typedRef = useRef<string | null>(null);
  const id = useId();

  const setText = (text: string | null) => {
    typedRef.current = text;
    setTyped(text);
  };

  const commit = () => {
    const text = typedRef.current;
    setText(null);
    editor.setPendingField(null, false);
    const number = Number(text);
    if (text !== null && text.trim() !== "" && Number.isFinite(number) && text !== shown(value)) {
      onCommit(number);
    }
  };

  const cancel = () => {
    setText(null);
    editor.setPendingField(null, false);
  };

  const change = (text: string) => {
    setText(text);
    const number = Number(text);
    const differs = text.trim() !== "" && Number.isFinite(number) && Math.round(number) !== Math.round(value);
    editor.setPendingField(commit, differs);
  };

  return (
    <div className="pixel-field">
      <label htmlFor={id} className="pixel-label muted">
        {title}
      </label>
      <div className="pixel-input">
        <input
          id={id}
          inputMode="numeric"
          aria-label={`${title} pixels`}
          value={typed ?? shown(value)}
          onChange={(event) => change(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              commit();
            } else if (event.key === "Escape") {
              event.stopPropagation();
              cancel();
              event.currentTarget.blur();
            }
          }}
        />
        <span className="muted">px</span>
      </div>
    </div>
  );
}
