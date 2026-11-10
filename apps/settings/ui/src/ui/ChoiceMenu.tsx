import { useRef, useState } from "react";

import { CheckIcon, ChevronDownIcon } from "./icons";
import { Popover } from "./Popover";

export interface Choice<T extends string> {
  value: T;
  title: string;
}

interface ChoiceMenuProps<T extends string> {
  label: string;
  value: T;
  choices: readonly Choice<T>[];
  onSelect: (value: T) => void;
  disabled?: boolean;
  className?: string;
}

/** A button showing the current choice that opens a list of the others. */
export function ChoiceMenu<T extends string>({
  label,
  value,
  choices,
  onSelect,
  disabled = false,
  className = "",
}: ChoiceMenuProps<T>) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const current = choices.find((choice) => choice.value === value)?.title ?? "";

  return (
    <div className={`choice-menu ${className}`} ref={anchor}>
      <button
        type="button"
        className="choice-button"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen(!open)}
      >
        <span className="choice-value">{current}</span>
        <ChevronDownIcon />
      </button>
      {open && (
        <Popover
          label={label}
          role="listbox"
          anchor={anchor}
          onClose={() => setOpen(false)}
          className="choice-list"
        >
          {choices.map((choice) => (
            <button
              type="button"
              role="option"
              key={choice.value}
              aria-selected={choice.value === value}
              className="choice-option"
              onClick={() => {
                setOpen(false);
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
