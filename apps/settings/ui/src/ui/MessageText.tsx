import { Fragment } from "react";

import { splitAtShortcuts } from "../core/labels";
import type { NamedShortcut } from "../core/model";
import { ChordText } from "./Chord";
import { Reference, ShortcutReference } from "./Reference";

/**
 * A message with each chord it names drawn as a reference, and `shortcut`, where the message
 * writes its title, as a reference that goes to that shortcut.
 */
export function MessageText({ text, shortcut = null }: { text: string; shortcut?: NamedShortcut | null }) {
  const at = shortcut === null ? -1 : text.lastIndexOf(shortcut.title);
  if (shortcut === null || at < 0) {
    return (
      <span>
        <WithShortcuts text={text} />
      </span>
    );
  }
  return (
    <span>
      <WithShortcuts text={text.slice(0, at)} />
      <ShortcutReference shortcut={shortcut} />
      <WithShortcuts text={text.slice(at + shortcut.title.length)} />
    </span>
  );
}

function WithShortcuts({ text }: { text: string }) {
  return splitAtShortcuts(text).map((piece, index) => (
    <Fragment key={index}>
      {typeof piece === "string" ? (
        piece
      ) : (
        <Reference>
          <ChordText snappyKey={piece.snappyKey} keys={piece.keys} label={piece.long} />
        </Reference>
      )}
    </Fragment>
  ));
}
