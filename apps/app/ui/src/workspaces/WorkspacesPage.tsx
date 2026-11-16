import { useEffect, useState } from "react";

import { useAppState, useSend } from "../state/services";
import { Toggle } from "../ui/Toggle";
import { WorkspaceDetail } from "./WorkspaceDetail";
import { WorkspaceList } from "./WorkspaceList";

/** The Workspaces page. */
export function WorkspacesPage() {
  const { settings, addedWorkspace } = useAppState();
  const send = useSend();
  const workspaces = settings.workspaces;
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // A new workspace is selected.
  useEffect(() => {
    if (addedWorkspace) {
      setSelectedId(addedWorkspace);
    }
  }, [addedWorkspace]);

  const selected =
    workspaces.items.find((item) => item.id === selectedId) ??
    workspaces.items.find((item) => item.id === workspaces.activeId) ??
    workspaces.items[0];

  return (
    <div className="workspaces-page">
      <div className="page-heading">
        <div>
          <h1 className="page-title">Workspaces</h1>
          <p className="page-subtitle muted">Switch between window groups and their layouts.</p>
        </div>
        <div className="workspaces-switch">
          <Toggle
            label="Enable workspaces"
            checked={workspaces.enabled}
            onChange={(enabled) => void send({ kind: "setWorkspacesEnabled", enabled })}
          />
        </div>
      </div>
      <div className="workspaces-body">
        <WorkspaceList selectedId={selected?.id ?? null} onSelect={setSelectedId} />
        {selected && <WorkspaceDetail workspace={selected} onDeleted={() => setSelectedId(null)} />}
      </div>
    </div>
  );
}
