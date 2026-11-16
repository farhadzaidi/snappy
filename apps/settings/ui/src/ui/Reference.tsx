import type { ReactNode } from "react";

/**
 * A shortcut or setting named inside a sentence, set apart in an outline on the text's line:
 * "Reset ⟦Move between zones⟧ shortcuts", "⟦[◧ Snappy] + 3⟧ is taken".
 */
export function Reference({ children }: { children: ReactNode }) {
  return <span className="reference">{children}</span>;
}
