import { useId } from "react";

import { modifierLabel } from "../core/labels";
import type { DragModifier, SwapModifier, WhenTaken } from "../core/model";
import { useAppState, useSend } from "../state/services";
import { ChoiceMenu } from "../ui/ChoiceMenu";
import { Group, Row } from "../ui/Group";
import { Toggle } from "../ui/Toggle";

const DRAG_MODIFIERS: readonly DragModifier[] = ["shift", "alt", "ctrl"];
const SWAP_MODIFIERS: readonly SwapModifier[] = ["meta", "ctrl", "alt", "shift", "off"];
const WHEN_TAKEN: readonly { value: WhenTaken; title: string }[] = [
  { value: "swap", title: "Swap" },
  { value: "stack", title: "Stack on top" },
];

/** What the reverse key does, which is the opposite of what a drop into a taken zone does. */
const REVERSE_KEY_HINTS: Record<WhenTaken, string> = {
  swap: "Hold while dropping to stack instead of swapping",
  stack: "Hold while dropping to swap instead of stacking",
};

/** The drag key, what happens in a taken zone, the key that reverses it, and the switch for every shortcut. */
export function SnappingGroup() {
  const { settings, platform } = useAppState();
  const send = useSend();
  const reverseHint = useId();
  const { dragModifier, swapModifier, whenTaken, shortcutsEnabled } = settings.snapping;

  return (
    <Group
      title="Snapping"
      caption="Hold a key while dragging a window to snap it into a zone."
      info="Zones appear on the display under the pointer. Let go of the key before dropping to cancel."
    >
      <Row label="Hold to snap">
        <ChoiceMenu
          label="Hold to snap"
          value={dragModifier}
          choices={DRAG_MODIFIERS.map((modifier) => ({
            value: modifier,
            title: modifierLabel(modifier, platform),
          }))}
          onSelect={(modifier) => void send({ kind: "setDragModifier", modifier })}
          className="setting-menu"
        />
      </Row>
      <Row label="When the zone is taken">
        <ChoiceMenu
          label="When the zone is taken"
          value={whenTaken}
          choices={WHEN_TAKEN}
          onSelect={(choice) => void send({ kind: "setWhenTaken", choice })}
          className="setting-menu"
        />
      </Row>
      <Row label="Reverse key" hint={REVERSE_KEY_HINTS[whenTaken]} hintId={reverseHint}>
        <ChoiceMenu
          label="Reverse key"
          describedBy={reverseHint}
          value={swapModifier}
          choices={SWAP_MODIFIERS.filter((modifier) => modifier !== dragModifier).map((modifier) => ({
            value: modifier,
            title: modifierLabel(modifier, platform),
          }))}
          onSelect={(modifier) => void send({ kind: "setSwapModifier", modifier })}
          className="setting-menu"
        />
      </Row>
      <Toggle
        label="Keyboard shortcuts"
        checked={shortcutsEnabled}
        onChange={(enabled) => void send({ kind: "setShortcutsEnabled", enabled })}
      />
    </Group>
  );
}
