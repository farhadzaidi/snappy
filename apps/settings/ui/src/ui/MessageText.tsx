import { Fragment } from "react";

import { splitAtShortcuts } from "../core/labels";
import { ChordText } from "./Chord";
import { Reference, SettingReference } from "./Reference";

/**
 * A message with each shortcut it names drawn as a reference, and the setting titled
 * `setting`, where the message names it, as a reference that goes to that setting.
 */
export function MessageText({ text, setting = null }: { text: string; setting?: string | null }) {
  const at = setting === null ? -1 : text.lastIndexOf(setting);
  if (setting === null || at < 0) {
    return (
      <span>
        <WithShortcuts text={text} />
      </span>
    );
  }
  return (
    <span>
      <WithShortcuts text={text.slice(0, at)} />
      <SettingReference title={setting} />
      <WithShortcuts text={text.slice(at + setting.length)} />
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
