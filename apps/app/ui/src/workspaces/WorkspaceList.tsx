import { useId, useState, type DragEvent, type KeyboardEvent } from "react";

import { longLabel, modifierName } from "../core/labels";
import { coreLimits } from "../core/limits";
import type { Workspace } from "../core/model";
import { useAppState, useSend } from "../state/services";
import { ChordLabel } from "../ui/Chord";
import { PlusIcon } from "../ui/icons";

interface WorkspaceListProps {
  selectedId: string | null;
  onSelect: (workspaceId: string) => void;
}

/**
 * A card per workspace in saved order. Cards reorder by dragging, or with Alt (Option on
 * macOS) and the Up or Down arrow on a focused card, which each card's description says.
 */
export function WorkspaceList({ selectedId, onSelect }: WorkspaceListProps) {
  const { settings, platform } = useAppState();
  const send = useSend();
  const items = settings.workspaces.items;
  const [dragged, setDragged] = useState<number | null>(null);
  const hintId = useId();
  const alt = modifierName("alt", platform);

  /** `to` is the position, in the current order, the moved card lands before. */
  const move = (from: number, to: number) => {
    if (to !== from && to !== from + 1 && to >= 0 && to <= items.length) {
      void send({ kind: "moveWorkspaces", from: [from], to });
    }
  };

  const dropTarget = (event: DragEvent<HTMLElement>, index: number) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return event.clientY > bounds.top + bounds.height / 2 ? index + 1 : index;
  };

  return (
    <div className="workspace-list">
      <span id={hintId} className="visually-hidden">
        Press {alt} and the Up or Down arrow to move this workspace.
      </span>
      <ul className="workspace-cards" aria-label="Workspaces" title="Drag workspaces to change their order.">
        {items.map((workspace, index) => (
          <li
            key={workspace.id}
            draggable
            onDragStart={(event) => {
              event.dataTransfer.effectAllowed = "move";
              event.dataTransfer.setData("text/plain", workspace.id);
              setDragged(index);
            }}
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              event.preventDefault();
              if (dragged !== null) {
                move(dragged, dropTarget(event, index));
              }
              setDragged(null);
            }}
            onDragEnd={() => setDragged(null)}
          >
            <WorkspaceCard
              workspace={workspace}
              selected={workspace.id === selectedId}
              describedBy={hintId}
              onSelect={() => onSelect(workspace.id)}
              onKeyDown={(event) => {
                if (event.altKey && event.key === "ArrowUp") {
                  event.preventDefault();
                  move(index, index - 1);
                } else if (event.altKey && event.key === "ArrowDown") {
                  event.preventDefault();
                  move(index, index + 2);
                }
              }}
            />
          </li>
        ))}
      </ul>
      <button
        type="button"
        className="new-workspace"
        disabled={items.length >= coreLimits().workspaces}
        onClick={() => void send({ kind: "addWorkspace" })}
      >
        <PlusIcon />
        New workspace
      </button>
    </div>
  );
}

interface WorkspaceCardProps {
  workspace: Workspace;
  selected: boolean;
  /** The id of the hint that says how to move the card with the keyboard. */
  describedBy: string;
  onSelect: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
}

function WorkspaceCard({ workspace, selected, describedBy, onSelect, onKeyDown }: WorkspaceCardProps) {
  const app = useAppState();
  const running = app.workspaces.running;
  const active = running && workspace.id === app.settings.workspaces.activeId;
  const windows = app.workspaces.windowCounts[workspace.id] ?? 0;
  const shortcut = workspace.switchShortcut;
  const windowCount = `${windows} ${windows === 1 ? "window" : "windows"}`;
  // Everything the card shows, as assistive technology says it.
  const spoken = [
    `Configure ${workspace.name}`,
    ...(active ? ["current workspace"] : []),
    ...(shortcut ? [longLabel(shortcut, app.platform)] : []),
    ...(running ? [windowCount] : []),
  ];

  return (
    <button
      type="button"
      className="workspace-card"
      aria-label={spoken.join(", ")}
      aria-pressed={selected}
      aria-describedby={describedBy}
      onClick={onSelect}
      onKeyDown={onKeyDown}
    >
      <span className="workspace-card-top">
        <span className="workspace-card-title">
          <span className="workspace-card-name">{workspace.name}</span>
          {active && <span className="status-dot" title="Current workspace" />}
        </span>
        {shortcut && <ChordLabel chord={shortcut} platform={app.platform} />}
      </span>
      {running && <span className="workspace-card-windows muted">{windowCount}</span>}
    </button>
  );
}
