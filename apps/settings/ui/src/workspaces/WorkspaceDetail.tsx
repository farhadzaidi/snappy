import { useState } from "react";

import { allLayouts, isEnabled } from "../core/layouts";
import type { KeyChord, Workspace } from "../core/model";
import { workspaceConflict, type WorkspaceSlot } from "../recorder/conflicts";
import { ShortcutRecorder } from "../recorder/ShortcutRecorder";
import { useAppState, useSend } from "../state/services";
import { Button } from "../ui/Button";
import { ChoiceMenu } from "../ui/ChoiceMenu";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { Group, Row } from "../ui/Group";
import { TrashIcon } from "../ui/icons";
import { WorkspaceName } from "./WorkspaceName";

const SHORTCUTS: readonly { slot: WorkspaceSlot; title: string }[] = [
  { slot: "switchShortcut", title: "Switch to this workspace" },
  { slot: "sendShortcut", title: "Move window here" },
  { slot: "sendAndSwitchShortcut", title: "Move here and switch" },
];

/** The selected workspace's name, deleting, layouts, and shortcuts. */
export function WorkspaceDetail({ workspace, onDeleted }: { workspace: Workspace; onDeleted: () => void }) {
  const app = useAppState();
  const send = useSend();
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const workspaces = app.settings.workspaces;
  const running = app.workspaces.running;
  const update = (changed: Workspace) => void send({ kind: "updateWorkspace", workspace: changed });

  return (
    <div className="workspace-detail">
      <div className="workspace-detail-heading">
        <WorkspaceName
          key={workspace.id}
          workspace={workspace}
          onRename={(name) => update({ ...workspace, name })}
        />
        {running && workspace.id === workspaces.activeId && <span className="mint">Current</span>}
        <Button
          aria-label="Delete workspace"
          title={`Delete ${workspace.name}`}
          disabled={workspaces.items.length < 2}
          onClick={() => setConfirmingDelete(true)}
        >
          Delete
        </Button>
      </div>

      <WorkspacesStatus />

      <Group title="Layouts">
        {app.displays.map((display) => (
          <Row key={display.id} label={<span className="muted">{display.name}</span>}>
            <ChoiceMenu
              label={`Layout on ${display.name}`}
              value={layoutOn(workspace, display.id)}
              choices={allLayouts(app.settings)
                .filter((layout) => isEnabled(app.settings, layout))
                .map((layout) => ({ value: layout.id, title: layout.name }))}
              onSelect={(layoutId) =>
                update({ ...workspace, displays: { ...workspace.displays, [display.id]: layoutId } })
              }
              className="wide-menu"
            />
          </Row>
        ))}
      </Group>

      <Group title="Shortcuts">
        {SHORTCUTS.map(({ slot, title }) => (
          <Row key={slot} label={<span className="muted">{title}</span>}>
            <ShortcutRecorder
              title={title}
              chord={workspace[slot]}
              conflictOf={(chord: KeyChord) =>
                workspaceConflict(app.settings, workspace, slot, chord, app.platform)
              }
              onChange={(chord) => update({ ...workspace, [slot]: chord })}
            />
          </Row>
        ))}
      </Group>

      {confirmingDelete && (
        <ConfirmDialog
          title={`Delete ${workspace.name}?`}
          message={`Its windows will be moved to ${deletionDestination(workspace)}. Your windows and layouts will be kept.`}
          confirmTitle="Delete workspace"
          icon={<TrashIcon />}
          onCancel={() => setConfirmingDelete(false)}
          onConfirm={() => {
            setConfirmingDelete(false);
            onDeleted();
            void send({ kind: "deleteWorkspace", workspace: workspace.id });
          }}
        />
      )}
    </div>
  );

  /** The layout this workspace uses on a display: its own, else the display's, else the default. */
  function layoutOn(item: Workspace, displayId: string): string {
    return (
      item.displays[displayId] ??
      app.settings.layouts.displays[displayId] ??
      app.settings.layouts.defaultLayoutId
    );
  }

  /**
   * Where a deleted workspace's windows go: the active workspace, or the first other one
   * when deleting the active one.
   */
  function deletionDestination(item: Workspace): string {
    const destination =
      item.id === workspaces.activeId
        ? workspaces.items.find((other) => other.id !== item.id)
        : workspaces.items.find((other) => other.id === workspaces.activeId);
    return destination?.name ?? "";
  }
}

/** Why workspaces aren't running. */
function WorkspacesStatus() {
  const app = useAppState();
  if (!app.settings.workspaces.enabled) {
    return (
      <p className="muted">
        Enable workspaces to start. Your open windows will stay in the current workspace.
      </p>
    );
  }
  if (!app.workspaces.running) {
    return <p className="muted">{app.workspaces.pauseMessage ?? "Waiting for Accessibility access."}</p>;
  }
  return null;
}
