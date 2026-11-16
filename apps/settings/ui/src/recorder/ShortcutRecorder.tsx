import { useEffect, useId, useRef, useState } from "react";

import { longLabel, recorderHelp } from "../core/labels";
import type { KeyChord, Refusal } from "../core/model";
import { core } from "../core/snappyCore";
import { useObservable } from "../state/observable";
import { useAppState, useServices } from "../state/services";
import { IconButton } from "../ui/Button";
import { ChordLabel } from "../ui/Chord";
import { CloseIcon } from "../ui/icons";
import { interpretKey } from "./interpretKey";
import { useRecordingInput } from "./useRecordingInput";

interface ShortcutRecorderProps {
  /** Names the shortcut for assistive technology and the clear button. */
  title: string;
  chord: KeyChord | null;
  /** The conflict this chord would create, checked locally before the probe. */
  conflictOf: (chord: KeyChord) => Refusal | null;
  /** Without a handler the recorder only shows its shortcut. */
  onChange?: (chord: KeyChord | null) => void;
}

/**
 * Records one shortcut. Only one recorder records at a time, and while it does the daemon
 * has every Snappy shortcut unregistered. Recording ends on a result, on Escape or Cancel,
 * on a press elsewhere, when another recorder starts, or when the recorder goes away.
 */
export function ShortcutRecorder({ title, chord, conflictOf, onChange }: ShortcutRecorderProps) {
  const { client, recording: recordings } = useServices();
  const { platform, settings } = useAppState();
  const id = useId();
  const recording = useObservable(recordings.active) === id;
  const [hint, setHint] = useState<string | null>(null);
  /** The candidate whose probe the recorder waits for. Its answer counts only while it is. */
  const probing = useRef<KeyChord | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const descriptionId = useId();

  const begin = () => {
    if (!onChange) {
      return;
    }
    setHint(null);
    probing.current = null;
    recordings.start(id);
    // WebKit doesn't focus a clicked button; assistive technology should land on it anyway.
    button.current?.focus();
  };
  const finish = () => recordings.stop(id);

  useEffect(() => () => recordings.stop(id), [recordings, id]);

  const refuse = ({ message, setting }: Refusal) => {
    setHint(message);
    client.announce(message, true, setting);
  };

  const tryCandidate = async (candidate: KeyChord) => {
    const problem = core.chordProblem(candidate, platform);
    if (problem !== null) {
      refuse({ message: problem, setting: null });
      return;
    }
    const conflict = conflictOf(candidate);
    if (conflict !== null) {
      refuse(conflict);
      return;
    }
    probing.current = candidate;
    const available = await client.probe(candidate);
    if (probing.current !== candidate) {
      return;
    }
    probing.current = null;
    if (!recordings.isRecording(id)) {
      return;
    }
    if (!available) {
      refuse(core.unavailableShortcut(candidate, title, platform));
      return;
    }
    setHint(null);
    onChange?.(candidate);
    finish();
  };

  useRecordingInput(recording, root, {
    onLeave: finish,
    onKey: (event) => {
      const outcome = interpretKey(event, settings.snappyKey);
      // While a probe is out, only Escape counts, so a probe that never answers can't keep
      // the recorder from ending.
      if (probing.current !== null && outcome.kind !== "cancel") {
        return;
      }
      switch (outcome.kind) {
        case "cancel":
          probing.current = null;
          finish();
          break;
        case "clear":
          onChange?.(null);
          finish();
          break;
        case "candidate":
          void tryCandidate(outcome.chord);
          break;
        case "ignore":
          break;
      }
    },
  });

  /** The shortcut the button shows, or null while it asks for one. */
  const shown = recording ? null : chord;
  const prompt = recording ? "Press shortcut…" : "Record shortcut";
  const valueText = shown ? longLabel(shown, platform) : prompt;

  return (
    <div className="recorder" ref={root}>
      <div className="recorder-field">
        <button
          ref={button}
          type="button"
          className="recorder-button"
          data-setting={title}
          data-recording={recording}
          data-assigned={chord !== null}
          aria-label={title}
          aria-describedby={descriptionId}
          title={hint ?? recorderHelp(platform, settings.snappyKey)}
          disabled={!onChange}
          onClick={begin}
        >
          {shown ? <ChordLabel chord={shown} platform={platform} /> : prompt}
        </button>
        {recording && (
          <button
            type="button"
            className="recorder-cancel"
            aria-label="Cancel recording"
            title="Cancel (Esc)"
            onClick={finish}
          >
            esc
          </button>
        )}
      </div>
      <span id={descriptionId} className="visually-hidden">
        {hint ?? valueText}
      </span>
      {chord && onChange && !recording ? (
        <IconButton label={`Clear ${title}`} className="recorder-clear" onClick={() => onChange(null)}>
          <CloseIcon />
        </IconButton>
      ) : (
        <span className="recorder-clear" aria-hidden="true" />
      )}
    </div>
  );
}
