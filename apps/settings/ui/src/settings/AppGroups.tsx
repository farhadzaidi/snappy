// The groups about Snappy itself: starting at login and its access, backups, and the
// debug log.

import { useAppState, useSend, useServices } from "../state/services";
import { Button } from "../ui/Button";
import { Group, Row } from "../ui/Group";
import { StatusBadge } from "../ui/StatusBadge";
import { Toggle } from "../ui/Toggle";

/** The login item, and Accessibility where the platform has a permission step. */
export function StartupGroup() {
  const { loginItemEnabled, snapping } = useAppState();
  const send = useSend();
  return (
    <Group
      title="Startup & access"
      caption={
        snapping.needsPermission ? "Open Snappy at login, and check its access." : "Open Snappy at login."
      }
      info={
        snapping.needsPermission
          ? "Allow Snappy in System Settings → Privacy & Security → Accessibility."
          : "Snappy starts in the background when you log in."
      }
    >
      <Toggle
        label="Open at login"
        checked={loginItemEnabled}
        onChange={(enabled) => void send({ kind: "setLoginItem", enabled })}
      />
      {snapping.needsPermission && (
        <Row label="Accessibility">
          <StatusBadge good={snapping.permissionGranted}>
            {snapping.permissionGranted ? "Allowed" : "Not allowed"}
          </StatusBadge>
          {!snapping.permissionGranted && (
            <Button onClick={() => void send({ kind: "setSnapping", enabled: true })}>Check access</Button>
          )}
        </Row>
      )}
    </Group>
  );
}

/** Import and export through the system file pickers. */
export function BackupGroup() {
  const { bridge } = useServices();
  const send = useSend();

  const importSettings = async () => {
    const path = await bridge.files.chooseImportPath();
    if (path) {
      void send({ kind: "importSettings", path });
    }
  };
  const exportSettings = async () => {
    const path = await bridge.files.chooseExportPath();
    if (path) {
      void send({ kind: "exportSettings", path });
    }
  };

  return (
    <Group
      title="Backup & restore"
      caption="Save layouts and preferences to a file, or load them from one."
      info="Importing replaces your settings and turns workspaces off."
    >
      <div className="button-row">
        <Button onClick={() => void importSettings()}>Import settings</Button>
        <Button onClick={() => void exportSettings()}>Export settings</Button>
      </div>
    </Group>
  );
}

/** The debug log, for finding out why something didn't behave as expected. */
export function DebugGroup() {
  const { settings, debugLogPath } = useAppState();
  const send = useSend();
  return (
    <Group
      title="Debug"
      caption="Record what Snappy does, never window titles, to share when reporting a problem."
      info="Turn it on, reproduce the problem, then share the file below."
    >
      <Toggle
        label="Debug log"
        checked={settings.debugLog}
        onChange={(enabled) => void send({ kind: "setDebugLog", enabled })}
      />
      {settings.debugLog && <p className="muted debug-log-path">{debugLogPath}</p>}
    </Group>
  );
}
