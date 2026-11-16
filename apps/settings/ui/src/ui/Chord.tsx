import { Fragment } from "react";

import { chordKeys, labelParts, longLabel, SNAPPY_KEY_NAME } from "../core/labels";
import type { KeyChord, PlatformName } from "../core/model";
import type { LabelParts } from "../core/snappyCore";

/**
 * A shortcut drawn from its label's parts: the Snappy key's pill, then each key, modifiers
 * included, set apart from the lighter + between them, "[◧ Snappy] + Shift ⇧ + J", or only
 * keys, "Control ⌃ + Option ⌥ + L". Assistive technology reads `label` instead of the pieces.
 */
export function ChordText({ snappyKey, keys, label }: LabelParts & { label: string }) {
  return (
    <span className="chord" role="img" aria-label={label}>
      {snappyKey && (
        <>
          <SnappyKey />
          <ChordPlus />
        </>
      )}
      <span className="chord-keys">
        {chordKeys(keys).map((key, index) => (
          <Fragment key={index}>
            {index > 0 && <ChordPlus />}
            <span className="chord-key">{key}</span>
          </Fragment>
        ))}
      </span>
    </span>
  );
}

function ChordPlus() {
  return <span className="chord-plus">+</span>;
}

/** A chord drawn as `ChordText`, read by assistive technology as its long label. */
export function ChordLabel({ chord, platform }: { chord: KeyChord; platform: PlatformName }) {
  return <ChordText {...labelParts(chord, platform)} label={longLabel(chord, platform)} />;
}

/**
 * The Snappy key by name in its mint pill, with the app's glyph before it as macOS shows
 * "⌘ Command". It looks the same wherever it appears. The glyph is decorative; the word is
 * what assistive technology reads.
 */
export function SnappyKey() {
  return (
    <span className="snappy-key">
      <SnappyKeyGlyph />
      {SNAPPY_KEY_NAME}
    </span>
  );
}

/** The glyph from `assets/icon/snappy-glyph.svg`, set like a character of the text around it. */
function SnappyKeyGlyph() {
  return (
    <svg
      className="snappy-key-glyph"
      viewBox="0 0 16 16"
      fill="currentColor"
      focusable="false"
      aria-hidden="true"
    >
      <path
        fillRule="evenodd"
        d="M3.5,1 H12.5 A3.5,3.5 0 0 1 16,4.5 V11.5 A3.5,3.5 0 0 1 12.5,15 H3.5 A3.5,3.5 0 0 1 0,11.5 V4.5 A3.5,3.5 0 0 1 3.5,1 Z M3.5,2 H12.5 A2.5,2.5 0 0 1 15,4.5 V11.5 A2.5,2.5 0 0 1 12.5,14 H3.5 A2.5,2.5 0 0 1 1,11.5 V4.5 A2.5,2.5 0 0 1 3.5,2 Z"
      />
      <rect x="2" y="3" width="5" height="10" rx="1" />
      <rect x="9" y="3" width="5" height="4" rx="1" opacity="0.4" />
      <rect x="9" y="9" width="5" height="4" rx="1" opacity="0.4" />
    </svg>
  );
}
