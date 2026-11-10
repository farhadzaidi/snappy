import { useState } from "react";

import type { Workspace } from "../core/model";

/** Workspace names are 1 to 80 characters and not blank. */
const MAX_NAME_LENGTH = 80;

/**
 * The name commits on Enter or on leaving the field, trimmed; an invalid name
 * reverts.
 */
export function WorkspaceName({
  workspace,
  onRename,
}: {
  workspace: Workspace;
  onRename: (name: string) => void;
}) {
  const [typed, setTyped] = useState<string | null>(null);

  const commit = () => {
    if (typed === null) {
      return;
    }
    const name = typed.trim();
    setTyped(null);
    if (name !== "" && [...name].length <= MAX_NAME_LENGTH && name !== workspace.name) {
      onRename(name);
    }
  };

  return (
    <input
      className="workspace-name"
      aria-label="Workspace name"
      value={typed ?? workspace.name}
      onChange={(event) => setTyped(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.currentTarget.blur();
        }
      }}
    />
  );
}
