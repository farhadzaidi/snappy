import { useLayoutEffect, useState, type ReactNode } from "react";

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
 * A setting a message names, as a reference with an arrow that goes to it while it is on the
 * page, or only its name when it isn't.
 */
export function SettingReference({ title }: { title: string }) {
  const { page } = useSession();
  const [shown, setShown] = useState(false);
  useLayoutEffect(() => setShown(findSetting(title) !== null), [title, page]);

  if (!shown) {
    return <Reference>{title}</Reference>;
  }
  return (
    <button
      type="button"
      className="reference reference-link"
      aria-label={`Go to ${title}`}
      title={`Go to ${title}`}
      onClick={() => revealSetting(title)}
    >
      {title}
      <ArrowIcon direction="right" />
    </button>
  );
}

/** The control for the setting titled `title`, which carries it as `data-setting`. */
function findSetting(title: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-setting="${CSS.escape(title)}"]`);
}

/** Scroll the setting's row into view and focus its control. */
function revealSetting(title: string): void {
  const control = findSetting(title);
  if (!control) {
    return;
  }
  (control.closest<HTMLElement>(".row") ?? control).scrollIntoView({ behavior: "smooth", block: "center" });
  control.focus({ preventScroll: true });
}
