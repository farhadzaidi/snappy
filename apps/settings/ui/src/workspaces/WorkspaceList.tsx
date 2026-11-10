import { useState, type DragEvent, type KeyboardEvent } from "react";

import { compactLabel } from "../core/labels";
import type { Workspace } from "../core/model";
import { MAX_WORKSPACES } from "../core/model";
import { useAppState, useSend } from "../state/services";
import { Button } from "../ui/Button";
import { PlusIcon } from "../ui/icons";

interface WorkspaceListProps {
  selectedId: string | null;
  onSelect: (workspaceId: string) => void;
}

/**
 * A card per workspace in saved order. Cards reorder by dragging, or with
 * Option+Up and Option+Down on a focused card.
 */
export function WorkspaceList({ selectedId, onSelect }: WorkspaceListProps) {
  const { settings } = useAppState();
  const send = useSend();
  const items = settings.workspaces.items;
  const [dragged, setDragged] = useState<number | null>(null);

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
      <Button
        className="new-workspace"
        disabled={items.length >= MAX_WORKSPACES}
        onClick={() => void send({ kind: "addWorkspace" })}
      >
        <PlusIcon />
        New workspace
      </Button>
    </div>
  );
}

interface WorkspaceCardProps {
  workspace: Workspace;
  selected: boolean;
  onSelect: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
}

function WorkspaceCard({ workspace, selected, onSelect, onKeyDown }: WorkspaceCardProps) {
  const app = useAppState();
  const running = app.workspaces.running;
  const active = running && workspace.id === app.settings.workspaces.activeId;
  const windows = app.workspaces.windowCounts[workspace.id] ?? 0;
  const shortcut = workspace.switchShortcut;

  return (
    <button
      type="button"
      className="workspace-card"
      aria-label={`Configure ${workspace.name}`}
      aria-pressed={selected}
      onClick={onSelect}
      onKeyDown={onKeyDown}
    >
      <span className="workspace-card-top">
        <span className="workspace-card-name">{workspace.name}</span>
        {active && <span className="status-dot" title="Current workspace" />}
      </span>
      <span className="workspace-card-bottom">
        {running && <span className="muted">{windows} windows</span>}
        {shortcut && <span className="mint">{compactLabel(shortcut, app.platform)}</span>}
      </span>
    </button>
  );
}
