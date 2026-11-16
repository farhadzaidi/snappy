import { useLayoutEffect, useState, type ReactNode } from "react";

import type { NamedShortcut } from "../core/model";
import { shortcutKey } from "../core/shortcuts";
import { useSession } from "../state/services";
import { ArrowIcon } from "./icons";

/**
 * A shortcut or setting named inside a sentence, set apart in an outline on the text's line:
 * "Reset ⟦Move between zones⟧ shortcuts", "⟦[◧ Snappy] + 3⟧ is taken".
 */
export function Reference({ children }: { children: ReactNode }) {
  return <span className="reference">{children}</span>;
}

/**
 * A shortcut a message names, as a reference with an arrow that goes to its recorder while
 * that is on the page, or only its title when it isn't.
 */
export function ShortcutReference({ shortcut }: { shortcut: NamedShortcut }) {
  const { page } = useSession();
  const key = shortcutKey(shortcut.id);
  const [shown, setShown] = useState(false);
  useLayoutEffect(() => setShown(findRecorder(key) !== null), [key, page]);

  if (!shown) {
    return <Reference>{shortcut.title}</Reference>;
  }
  return (
    <button
      type="button"
      className="reference reference-link"
      aria-label={`Go to ${shortcut.title}`}
      title={`Go to ${shortcut.title}`}
      onClick={() => revealRecorder(key)}
    >
      {shortcut.title}
      <ArrowIcon direction="right" />
    </button>
  );
}

/** The recorder carrying `key`, its shortcut's `shortcutKey`, as `data-shortcut`. */
function findRecorder(key: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-shortcut="${CSS.escape(key)}"]`);
}

/** Scroll the recorder's row into view and focus it. */
function revealRecorder(key: string): void {
  const recorder = findRecorder(key);
  if (!recorder) {
    return;
  }
  (recorder.closest<HTMLElement>(".row") ?? recorder).scrollIntoView({ behavior: "smooth", block: "center" });
  recorder.focus({ preventScroll: true });
}
