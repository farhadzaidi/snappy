import { type ReactNode, useState } from "react";

import { modifierLabel, modifierName, snappyKeyLabel } from "../core/labels";
import {
  DIRECTIONS,
  type DragModifier,
  type FocusShortcut,
  type HistoryStep,
  type KeyChord,
  type Modifier,
  type NavigationShortcut,
  type SwapModifier,
  type WhenTaken,
  type WorkspaceStep,
} from "../core/model";
import { core } from "../core/snappyCore";
import { historyConflict, navigationConflict, workspaceStepConflict } from "../recorder/conflicts";
import { ShortcutRecorder } from "../recorder/ShortcutRecorder";
import { useAppState, useSend, useServices } from "../state/services";
import { Button } from "../ui/Button";
import { ChoiceMenu } from "../ui/ChoiceMenu";
import { Group, Row } from "../ui/Group";
import { AlertIcon, ArrowIcon } from "../ui/icons";
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
        <SnappyKeyGroup />
        <MoveBetweenZonesGroup />
        <FocusAndSelectionGroup />
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

/** The modifiers the Snappy key may hold, in the order labels show them. */
const SNAPPY_KEY_MODIFIERS: readonly Modifier[] = ["ctrl", "alt", "meta", "shift"];
const DEFAULT_SNAPPY_KEY: readonly Modifier[] = ["ctrl", "alt", "meta"];

function sameModifiers(a: readonly Modifier[], b: readonly Modifier[]): boolean {
  return a.length === b.length && a.every((modifier) => b.includes(modifier));
}

/**
 * The modifiers the Snappy key stands for. Every shortcut stored with it follows a change,
 * which the daemon refuses, saying what clashed, unless every one of them still works.
 */
function SnappyKeyGroup() {
  const { settings, platform, capsLockProblem } = useAppState();
  const send = useSend();
  const [refusal, setRefusal] = useState<string | null>(null);
  const [capsLockRefusal, setCapsLockRefusal] = useState<string | null>(null);
  const snappyKey = settings.snappyKey;
  const capsLockNotice = capsLockRefusal ?? (settings.capsLockAsSnappyKey ? capsLockProblem : null);

  const choose = async (modifiers: Modifier[]) => {
    setRefusal(await send({ kind: "setSnappyKey", modifiers }));
  };
  const toggle = (modifier: Modifier) =>
    choose(
      snappyKey.includes(modifier)
        ? snappyKey.filter((held) => held !== modifier)
        : SNAPPY_KEY_MODIFIERS.filter((held) => held === modifier || snappyKey.includes(held)),
    );

  return (
    <Group
      title="Snappy key"
      caption={`Shortcuts shown as Snappy hold the Snappy key, ${snappyKeyLabel(snappyKey, platform)}. Choose at least two modifiers.`}
    >
      <Row label={<span className="muted">Modifiers</span>}>
        <div className="segmented" role="group" aria-label="Snappy key modifiers">
          {SNAPPY_KEY_MODIFIERS.map((modifier) => (
            <button
              type="button"
              key={modifier}
              className="segment"
              aria-pressed={snappyKey.includes(modifier)}
              onClick={() => void toggle(modifier)}
            >
              {modifierName(modifier, platform)}
            </button>
          ))}
        </div>
      </Row>
      {refusal !== null && (
        <p className="notice notice-warning" role="alert">
          <AlertIcon />
          {refusal}
        </p>
      )}
      <div className="button-row">
        <Button
          disabled={sameModifiers(snappyKey, DEFAULT_SNAPPY_KEY)}
          onClick={() => void choose([...DEFAULT_SNAPPY_KEY])}
        >
          Reset to default
        </Button>
      </div>
      <Toggle
        label="Use Caps Lock as the Snappy key"
        checked={settings.capsLockAsSnappyKey}
        onChange={(enabled) =>
          void send({ kind: "setCapsLockAsSnappyKey", enabled }).then(setCapsLockRefusal)
        }
      />
      <p className="muted caps-lock-hint">
        Hold Caps Lock instead of {snappyKeyLabel(snappyKey, platform)}. Tap it alone to turn Caps Lock on or
        off.
      </p>
      {capsLockNotice !== null && (
        <p className="notice notice-warning" role="alert">
          <AlertIcon />
          {capsLockNotice}
        </p>
      )}
    </Group>
  );
}

/** A row recording one shortcut of the navigation section, with `icon` before its title. */
function NavigationRow({ shortcut, icon }: { shortcut: NavigationShortcut; icon?: ReactNode }) {
  const { settings, platform } = useAppState();
  const send = useSend();
  const title = core.shortcutTitles().navigation[shortcut];
  return (
    <Row
      label={
        <span className={icon === undefined ? "muted" : "direction-label muted"}>
          {icon}
          {title}
        </span>
      }
    >
      <ShortcutRecorder
        title={title}
        chord={settings.navigation[shortcut]}
        conflictOf={(chord: KeyChord) => navigationConflict(settings, shortcut, chord, platform)}
        onChange={(chord) => void send({ kind: "setNavigationShortcut", shortcut, chord })}
      />
    </Row>
  );
}

/** Move between zones: a recorder for each direction, and one for the nearest empty zone. */
function MoveBetweenZonesGroup() {
  return (
    <Group
      title="Move between zones"
      caption="Move the focused window to the next zone in that direction, or into the nearest empty zone."
    >
      {DIRECTIONS.map((direction) => (
        <NavigationRow key={direction} shortcut={direction} icon={<ArrowIcon direction={direction} />} />
      ))}
      <NavigationRow shortcut="nearestEmpty" />
    </Group>
  );
}

const FOCUS_SHORTCUTS: readonly FocusShortcut[] = [
  "focusUp",
  "focusLeft",
  "focusDown",
  "focusRight",
  "selectWindow",
  "clearSelection",
];

/** Moving focus to the next window in a direction, and selecting windows. */
function FocusAndSelectionGroup() {
  return (
    <Group
      title="Focus and selection"
      caption="Move focus to the next window in a direction, from zone to zone when windows are in zones. Select windows to move them together: the nearest-empty-zone shortcut places them into empty zones, and switching or sending to a workspace takes them along."
    >
      {FOCUS_SHORTCUTS.map((shortcut) => (
        <NavigationRow key={shortcut} shortcut={shortcut} />
      ))}
    </Group>
  );
}

const WORKSPACE_STEPS: readonly WorkspaceStep[] = ["previous", "next", "sendPrevious", "sendNext"];

/** The shortcuts for every workspace: stepping, and sending windows away. */
function WorkspaceStepsGroup() {
  const { settings, platform } = useAppState();
  const send = useSend();
  const titles = core.shortcutTitles().workspaceSteps;
  return (
    <Group
      title="Workspaces"
      caption="Go to the previous or next workspace in list order, or send the focused window, or the selected windows, there and stay."
    >
      {WORKSPACE_STEPS.map((step) => (
        <Row key={step} label={<span className="muted">{titles[step]}</span>}>
          <ShortcutRecorder
            title={titles[step]}
            chord={settings.workspaces[step]}
            conflictOf={(chord: KeyChord) => workspaceStepConflict(settings, step, chord, platform)}
            onChange={(chord) => void send({ kind: "setWorkspaceStepShortcut", step, chord })}
          />
        </Row>
      ))}
    </Group>
  );
}

const HISTORY_STEPS: readonly HistoryStep[] = ["undo", "redo"];

/** Shortcuts to undo and redo window moves. Neither has a default. */
function WindowHistoryGroup() {
  const { settings, platform } = useAppState();
  const send = useSend();
  const titles = core.shortcutTitles().history;
  return (
    <Group title="Window history" caption="Undo or redo the last window move.">
      {HISTORY_STEPS.map((step) => (
        <Row key={step} label={<span className="muted">{titles[step]}</span>}>
          <ShortcutRecorder
            title={titles[step]}
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
