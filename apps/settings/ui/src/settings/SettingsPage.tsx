import { modifierLabel } from "../core/labels";
import {
  DIRECTIONS,
  type DragModifier,
  type HistoryStep,
  type KeyChord,
  type SwapModifier,
  type WhenTaken,
  type WorkspaceStep,
} from "../core/model";
import {
  historyConflict,
  navigationConflict,
  nearestEmptyConflict,
  workspaceStepConflict,
} from "../recorder/conflicts";
import { ShortcutRecorder } from "../recorder/ShortcutRecorder";
import { useAppState, useSend, useServices } from "../state/services";
import { Button } from "../ui/Button";
import { ChoiceMenu } from "../ui/ChoiceMenu";
import { Group, Row } from "../ui/Group";
import { ArrowIcon } from "../ui/icons";
import { Toggle } from "../ui/Toggle";

const DRAG_MODIFIERS: readonly DragModifier[] = ["shift", "alt", "ctrl"];
const SWAP_MODIFIERS: readonly SwapModifier[] = ["meta", "ctrl", "alt", "shift", "off"];
const WHEN_TAKEN: readonly { value: WhenTaken; title: string }[] = [
  { value: "swap", title: "Swap" },
  { value: "stack", title: "Stack on top" },
];

/** The Settings page. */
export function SettingsPage() {
  return (
    <div className="settings-page">
      <div className="settings-column">
        <h1 className="page-title">Global settings</h1>
        <SnappingGroup />
        <MoveBetweenZonesGroup />
        <WorkspaceStepsGroup />
        <WindowHistoryGroup />
        <StartupGroup />
        <BackupGroup />
        <TroubleshootingGroup />
      </div>
    </div>
  );
}

/** The drag key, what happens in an occupied zone, the key that does the opposite, and the switch for every shortcut. */
function SnappingGroup() {
  const { settings, platform } = useAppState();
  const send = useSend();
  const { dragModifier, swapModifier, whenTaken, shortcutsEnabled } = settings.snapping;
  const drag = modifierLabel(dragModifier, platform);
  const opposite =
    whenTaken === "swap" ? "stack on top instead of swapping" : "swap instead of stacking on top";
  const caption =
    swapModifier === "off"
      ? `Hold ${drag} while dragging a window.`
      : `Hold ${drag} while dragging a window. Add ${modifierLabel(swapModifier, platform)} to ${opposite}.`;

  return (
    <Group title="Snapping" caption={caption}>
      <Row label="Hold to snap">
        <ChoiceMenu
          label="Hold to snap"
          value={dragModifier}
          choices={DRAG_MODIFIERS.map((modifier) => ({
            value: modifier,
            title: modifierLabel(modifier, platform),
          }))}
          onSelect={(modifier) => void send({ kind: "setDragModifier", modifier })}
          className="setting-menu"
        />
      </Row>
      <Row label="When the zone is taken">
        <ChoiceMenu
          label="When the zone is taken"
          value={whenTaken}
          choices={WHEN_TAKEN}
          onSelect={(choice) => void send({ kind: "setWhenTaken", choice })}
          className="setting-menu"
        />
      </Row>
      <Row label="Hold to do the opposite">
        <ChoiceMenu
          label="Hold to do the opposite"
          value={swapModifier}
          choices={SWAP_MODIFIERS.filter((modifier) => modifier !== dragModifier).map((modifier) => ({
            value: modifier,
            title: modifierLabel(modifier, platform),
          }))}
          onSelect={(modifier) => void send({ kind: "setSwapModifier", modifier })}
          className="setting-menu"
        />
      </Row>
      <Toggle
        label="Keyboard shortcuts"
        checked={shortcutsEnabled}
        onChange={(enabled) => void send({ kind: "setShortcutsEnabled", enabled })}
      />
    </Group>
  );
}

const NEAREST_EMPTY_TITLE = "Move to the nearest empty zone";

/** Move between zones: a recorder for each direction, and one for the nearest empty zone. */
function MoveBetweenZonesGroup() {
  const { settings, platform } = useAppState();
  const send = useSend();
  return (
    <Group
      title="Move between zones"
      caption="Move the focused window to the next zone in that direction, or into the nearest empty zone."
    >
      {DIRECTIONS.map((direction) => {
        const title = `Move window ${direction}`;
        return (
          <Row
            key={direction}
            label={
              <span className="direction-label muted">
                <ArrowIcon direction={direction} />
                {title}
              </span>
            }
          >
            <ShortcutRecorder
              title={title}
              chord={settings.navigation[direction]}
              conflictOf={(chord: KeyChord) => navigationConflict(settings, direction, chord, platform)}
              onChange={(chord) => void send({ kind: "setNavigationShortcut", direction, chord })}
            />
          </Row>
        );
      })}
      <Row label={<span className="muted">{NEAREST_EMPTY_TITLE}</span>}>
        <ShortcutRecorder
          title={NEAREST_EMPTY_TITLE}
          chord={settings.navigation.nearestEmpty}
          conflictOf={(chord: KeyChord) => nearestEmptyConflict(settings, chord, platform)}
          onChange={(chord) => void send({ kind: "setNearestEmptyShortcut", chord })}
        />
      </Row>
    </Group>
  );
}

const WORKSPACE_STEPS: readonly { step: WorkspaceStep; title: string }[] = [
  { step: "previous", title: "Previous workspace" },
  { step: "next", title: "Next workspace" },
  { step: "movePrevious", title: "Move window to previous workspace" },
  { step: "moveNext", title: "Move window to next workspace" },
];

/** The previous and next workspace shortcuts, alone or taking the window along. */
function WorkspaceStepsGroup() {
  const { settings, platform } = useAppState();
  const send = useSend();
  return (
    <Group
      title="Workspaces"
      caption="Go to the previous or next workspace in list order, or take the focused window along."
    >
      {WORKSPACE_STEPS.map(({ step, title }) => (
        <Row key={step} label={<span className="muted">{title}</span>}>
          <ShortcutRecorder
            title={title}
            chord={settings.workspaces[step]}
            conflictOf={(chord: KeyChord) => workspaceStepConflict(settings, step, chord, platform)}
            onChange={(chord) => void send({ kind: "setWorkspaceStepShortcut", step, chord })}
          />
        </Row>
      ))}
    </Group>
  );
}

const HISTORY_STEPS: readonly { step: HistoryStep; title: string }[] = [
  { step: "undo", title: "Undo window move" },
  { step: "redo", title: "Redo window move" },
];

/** Shortcuts to undo and redo window moves. Neither has a default. */
function WindowHistoryGroup() {
  const { settings, platform } = useAppState();
  const send = useSend();
  return (
    <Group title="Window history" caption="Undo or redo the last window move.">
      {HISTORY_STEPS.map(({ step, title }) => (
        <Row key={step} label={<span className="muted">{title}</span>}>
          <ShortcutRecorder
            title={title}
            chord={settings.history[step]}
            conflictOf={(chord: KeyChord) => historyConflict(settings, step, chord, platform)}
            onChange={(chord) => void send({ kind: "setHistoryShortcut", step, chord })}
          />
        </Row>
      ))}
    </Group>
  );
}

/** The login item, and Accessibility where the platform has a permission step. */
function StartupGroup() {
  const { loginItemEnabled, snapping } = useAppState();
  const send = useSend();
  return (
    <Group
      title="Startup & access"
      caption={
        snapping.needsPermission ? "Open Snappy at login and check its access." : "Open Snappy at login."
      }
    >
      <Toggle
        label="Open at login"
        checked={loginItemEnabled}
        onChange={(enabled) => void send({ kind: "setLoginItem", enabled })}
      />
      {snapping.needsPermission && (
        <Row
          label={
            <span className="muted">
              {snapping.permissionGranted ? "Accessibility allowed" : "Accessibility required"}
            </span>
          }
        >
          {!snapping.live && (
            <Button onClick={() => void send({ kind: "setSnapping", enabled: true })}>Check access</Button>
          )}
        </Row>
      )}
    </Group>
  );
}

/** Import and export through the system file pickers. */
function BackupGroup() {
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
    <Group title="Backup & restore" caption="Save layouts and preferences to a file, or load them from one.">
      <div className="button-row">
        <Button onClick={() => void importSettings()}>Import settings…</Button>
        <Button onClick={() => void exportSettings()}>Export settings…</Button>
      </div>
    </Group>
  );
}

/** The debug log, for finding out why something didn't behave as expected. */
function TroubleshootingGroup() {
  const { settings, debugLogPath } = useAppState();
  const send = useSend();
  return (
    <Group
      title="Troubleshooting"
      caption="Record what Snappy does, then reproduce a problem and share the log. Window titles are never recorded."
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
