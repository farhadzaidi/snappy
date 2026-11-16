import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import { CheckIcon, ChevronDownIcon } from "./icons";
import { Popover } from "./Popover";

export interface Choice<T extends string> {
  value: T;
  title: string;
}

interface ChoiceMenuProps<T extends string> {
  label: string;
  /** The id of what describes the choice, such as a hint under its label. */
  describedBy?: string;
  value: T;
  choices: readonly Choice<T>[];
  onSelect: (value: T) => void;
  disabled?: boolean;
  className?: string;
}

/** A button showing the current choice that opens a list of the others. */
export function ChoiceMenu<T extends string>({
  label,
  describedBy,
  value,
  choices,
  onSelect,
  disabled = false,
  className = "",
}: ChoiceMenuProps<T>) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const current = choices.find((choice) => choice.value === value)?.title ?? "";

  const options = () => [...(anchor.current?.querySelectorAll<HTMLElement>('[role="option"]') ?? [])];

  // The list opens on the current choice, so the arrow keys start from it.
  useEffect(() => {
    if (open) {
      options()
        .find((option) => option.getAttribute("aria-selected") === "true")
        ?.focus();
    }
  }, [open]);

  const close = () => {
    setOpen(false);
    button.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (!open) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setOpen(true);
      }
      return;
    }
    const list = options();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const target = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: list.length - 1 }[event.key];
    if (target !== undefined) {
      event.preventDefault();
      list[(target + list.length) % list.length]?.focus();
    }
  };

  return (
    <div className={`choice-menu ${className}`} ref={anchor} onKeyDown={onKeyDown}>
      <button
        ref={button}
        type="button"
        className="choice-button"
        aria-label={label}
        aria-describedby={describedBy}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen(!open)}
      >
        <span className="choice-value">{current}</span>
        <ChevronDownIcon />
      </button>
      {open && (
        <Popover label={label} role="listbox" anchor={anchor} onClose={close} className="choice-list">
          {choices.map((choice) => (
            <button
              type="button"
              role="option"
              key={choice.value}
              aria-selected={choice.value === value}
              className="choice-option"
              onClick={() => {
                close();
                if (choice.value !== value) {
                  onSelect(choice.value);
                }
              }}
            >
              <span>{choice.title}</span>
              {choice.value === value && <CheckIcon />}
            </button>
          ))}
        </Popover>
      )}
    </div>
  );
}
