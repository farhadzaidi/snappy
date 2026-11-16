import { useId, useRef, useState, type ReactNode } from "react";

import { InfoIcon } from "./icons";
import { Popover } from "./Popover";

/**
 * An ⓘ button that shows more about `topic` while the pointer rests on it, and keeps it shown
 * once clicked, until it is clicked again, Escape is pressed, or something else is pressed.
 * Closing it by click stays closed until the pointer comes back.
 */
export function InfoHint({ topic, children }: { topic: string; children: ReactNode }) {
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const anchor = useRef<HTMLSpanElement>(null);
  const panelId = useId();
  const open = hovered || pinned;
  const label = `About ${topic}`;

  const close = () => {
    setHovered(false);
    setPinned(false);
  };

  return (
    <span
      className="info-hint"
      ref={anchor}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
    >
      <button
        type="button"
        className="info-hint-button"
        aria-label={label}
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => (pinned ? close() : setPinned(true))}
      >
        <InfoIcon />
      </button>
      {open && (
        <Popover
          id={panelId}
          label={label}
          role="note"
          anchor={anchor}
          onClose={close}
          className="info-hint-panel"
        >
          {children}
        </Popover>
      )}
    </span>
  );
}
