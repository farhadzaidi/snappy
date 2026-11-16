import { useState } from "react";

import { modifierName, snappyKeyLabel } from "../core/labels";
import type { Modifier, Refusal } from "../core/model";
import { core } from "../core/snappyCore";
import { WarningNotice } from "../shortcuts/WarningNotice";
import { useAppState, useSend } from "../state/services";
import { SnappyKey } from "../ui/Chord";
import { Group, Row } from "../ui/Group";
import { Reference } from "../ui/Reference";
import { ResetButton } from "../ui/ResetButton";
import { Toggle } from "../ui/Toggle";

/**
 * The modifiers the Snappy key may hold, in the order its buttons show them: the three of the
 * default, then Shift. Labels order them as `snappy-core` does.
 */
const SNAPPY_KEY_MODIFIERS: readonly Modifier[] = ["ctrl", "alt", "meta", "shift"];

/**
 * The modifiers the Snappy key stands for. Every shortcut stored with it follows a change,
 * which the daemon refuses, saying what clashed, unless every one of them still works.
 */
export function SnappyKeyGroup() {
  const { settings, platform, capsLockProblem, capsLockUnavailable } = useAppState();
  const send = useSend();
  const [refusal, setRefusal] = useState<Refusal | null>(null);
  const [capsLockRefusal, setCapsLockRefusal] = useState<Refusal | null>(null);
  const snappyKey = settings.snappyKey;
  const defaultKey = core.defaultSettings(platform).snappyKey;
  // Where the build can't hold Caps Lock, the switch is off and disabled with the reason under
  // it, whatever the stored setting says.
  const capsLockAvailable = capsLockUnavailable === null;
  const capsLockNotice = capsLockAvailable
    ? (capsLockRefusal?.message ?? (settings.capsLockAsSnappyKey ? capsLockProblem : null))
    : null;

  const choose = async (modifiers: Modifier[]) => {
    setRefusal(await send({ kind: "setSnappyKey", modifiers }, { inPlace: true }));
  };
  const toggle = (modifier: Modifier) =>
    choose(
      snappyKey.includes(modifier)
        ? snappyKey.filter((held) => held !== modifier)
        : SNAPPY_KEY_MODIFIERS.filter((held) => held === modifier || snappyKey.includes(held)),
    );

  return (
    <Group
      title="Snappy key"
      caption="The keys every Snappy shortcut holds."
      info="Choose at least two. Shortcuts that use it follow any change."
      action={
        <ResetButton
          label="Reset the Snappy key"
          question={
            <>
              Reset the{" "}
              <Reference>
                <SnappyKey /> key
              </Reference>{" "}
              to {snappyKeyLabel(defaultKey, platform)}?
            </>
          }
          disabled={core.changeSnappyKey(settings, defaultKey, platform) === "unchanged"}
          onConfirm={() => void choose(defaultKey)}
        />
      }
    >
      <Row label={<span className="muted">Modifiers</span>}>
        <div className="segmented segmented-even" role="group" aria-label="Snappy key modifiers">
          {SNAPPY_KEY_MODIFIERS.map((modifier) => (
            <button
              type="button"
              key={modifier}
              className="segment"
              aria-pressed={snappyKey.includes(modifier)}
              onClick={() => void toggle(modifier)}
            >
              {modifierName(modifier, platform)}
            </button>
          ))}
        </div>
      </Row>
      {refusal !== null && <WarningNotice message={refusal.message} shortcut={refusal.shortcut} />}
      <Toggle
        label="Also use Caps Lock as the Snappy key"
        hint={capsLockUnavailable ?? "A tap still toggles Caps Lock."}
        checked={capsLockAvailable && settings.capsLockAsSnappyKey}
        disabled={!capsLockAvailable}
        onChange={(enabled) =>
          void send({ kind: "setCapsLockAsSnappyKey", enabled }, { inPlace: true }).then(setCapsLockRefusal)
        }
      />
      {capsLockNotice !== null && <WarningNotice message={capsLockNotice} />}
    </Group>
  );
}
