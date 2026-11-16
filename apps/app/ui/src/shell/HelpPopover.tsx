import { useRef, useState, type ReactNode } from "react";

import { modifierLabel, snappyKeyLabel } from "../core/labels";
import { core } from "../core/snappyCore";
import { useAppState } from "../state/services";
import { IconButton } from "../ui/Button";
import { ChordLabel, ChordText, SnappyKey } from "../ui/Chord";
import { HelpIcon } from "../ui/icons";
import { Popover } from "../ui/Popover";

/** The "?" button and its popover. */
export function HelpButton() {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);

  return (
    <div className="help" ref={anchor}>
      <IconButton
        label="Shortcuts and help"
        className="help-button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <HelpIcon />
      </IconButton>
      {open && (
        <Popover
          label="Shortcuts and help"
          anchor={anchor}
          align="end"
          onClose={() => setOpen(false)}
          className="help-popover"
        >
          <HelpContent />
        </Popover>
      )}
    </div>
  );
}

function HelpContent() {
  const { settings, platform } = useAppState();
  const { dragModifier, swapModifier, whenTaken } = settings.snapping;
  const drag = modifierLabel(dragModifier, platform);
  const taken =
    whenTaken === "swap"
      ? "A window already in the zone trades places with it."
      : "The window covers any window already in the zone.";

  return (
    <div className="help-content">
      <h2 className="help-title">How to snap</h2>
      <p className="muted">
        Hold {drag}, drag a window, then release over a zone. {taken}
      </p>
      {swapModifier !== "off" && (
        <HelpRow
          title={whenTaken === "swap" ? "Stack on top while dragging" : "Swap while dragging"}
          keys={<Keys label={`${drag} + ${modifierLabel(swapModifier, platform)}`} />}
        />
      )}
      <hr className="help-divider" />
      <HelpRow
        title={
          <>
            <SnappyKey /> key
          </>
        }
        keys={<Keys label={snappyKeyLabel(settings.snappyKey, platform)} />}
      />
      {core.defaultShortcuts(platform).map(({ title, chord }) => (
        <HelpRow key={title} title={title} keys={<ChordLabel chord={chord} platform={platform} />} />
      ))}
      <HelpRow title="Hide the screen preview" keys={<Keys label="Escape" />} />
    </div>
  );
}

function HelpRow({ title, keys }: { title: ReactNode; keys: ReactNode }) {
  return (
    <div className="help-row">
      <span>{title}</span>
      <span className="mint">{keys}</span>
    </div>
  );
}

/** Keys without the Snappy key, "Shift ⇧ + Command ⌘", drawn as a shortcut is. */
function Keys({ label }: { label: string }) {
  return <ChordText snappyKey={false} keys={label} label={label} />;
}
