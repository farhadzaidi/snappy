import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";

import { commandModifierHint, compactLabel, recorderHelp, unavailableMessage } from "../core/labels";
import type { KeyChord } from "../core/model";
import { core } from "../core/snappyCore";
import { usePlatform, useServices } from "../state/services";
import { IconButton } from "../ui/Button";
import { CloseIcon } from "../ui/icons";
import { interpretKey } from "./interpretKey";

interface ShortcutRecorderProps {
  /** Names the shortcut for assistive technology and the clear button. */
  title: string;
  chord: KeyChord | null;
  /** The conflict this chord would create, checked locally before the probe. */
  conflictOf: (chord: KeyChord) => string | null;
  /** Without a handler the recorder only shows its shortcut. */
  onChange?: (chord: KeyChord | null) => void;
}

/**
 * Records one shortcut. While recording, the daemon has every Snappy
 * shortcut unregistered; recording ends on a result, on Escape, on leaving the recorder,
 * or when the recorder goes away.
 */
export function ShortcutRecorder({ title, chord, conflictOf, onChange }: ShortcutRecorderProps) {
  const { client } = useServices();
  const platform = usePlatform();
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  const recordingRef = useRef(false);
  const probing = useRef(false);
  const descriptionId = useId();

  const begin = () => {
    if (recordingRef.current || !onChange) {
      return;
    }
    recordingRef.current = true;
    setRecording(true);
    setHint(null);
    void client.send({ kind: "beginRecording" });
  };

  const finish = () => {
    if (!recordingRef.current) {
      return;
    }
    recordingRef.current = false;
    setRecording(false);
    void client.send({ kind: "endRecording" });
  };

  useEffect(
    () => () => {
      if (recordingRef.current) {
        void client.send({ kind: "endRecording" });
      }
    },
    [client],
  );

  const refuse = (message: string) => {
    setHint(message);
    client.announce(message, true);
  };

  const tryCandidate = async (candidate: KeyChord) => {
    const conflict = conflictOf(candidate);
    if (conflict !== null) {
      refuse(conflict);
      return;
    }
    probing.current = true;
    const available = await client.probe(candidate);
    probing.current = false;
    if (!recordingRef.current) {
      return;
    }
    if (!available) {
      refuse(unavailableMessage(candidate, platform));
      return;
    }
    setHint(null);
    onChange?.(candidate);
    finish();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recordingRef.current) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (probing.current) {
      return;
    }
    const outcome = interpretKey(event);
    switch (outcome.kind) {
      case "cancel":
        finish();
        break;
      case "clear":
        onChange?.(null);
        finish();
        break;
      case "needsCommandModifier":
        setHint(commandModifierHint(platform));
        break;
      case "candidate":
        if (core.isValidChord(outcome.chord, platform)) {
          void tryCandidate(outcome.chord);
        }
        break;
      case "ignore":
        break;
    }
  };

  const valueText = recording ? "Press shortcut…" : chord ? compactLabel(chord, platform) : "Record shortcut";

  return (
    <div className="recorder">
      <button
        type="button"
        className="recorder-button"
        data-recording={recording}
        data-assigned={chord !== null}
        aria-label={title}
        aria-describedby={descriptionId}
        title={hint ?? recorderHelp(platform)}
        disabled={!onChange}
        onClick={begin}
        onKeyDown={onKeyDown}
        onBlur={finish}
      >
        {valueText}
      </button>
      <span id={descriptionId} className="visually-hidden">
        {hint ?? valueText}
      </span>
      {chord && onChange ? (
        <IconButton label={`Clear ${title}`} className="recorder-clear" onClick={() => onChange(null)}>
          <CloseIcon />
        </IconButton>
      ) : (
        <span className="recorder-clear" aria-hidden="true" />
      )}
    </div>
  );
}
