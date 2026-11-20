// The Settings page's sections of global shortcuts, a group each with its own Reset. Which
// sections there are, their titles, and their shortcuts come from `snappy-core`; this file
// adds only what each says about itself and how a change reaches the daemon.

import { useId } from "react";

import type { Command } from "../bridge/protocol";
import { coreLimits } from "../core/limits";
import { DIRECTIONS, type Direction, type KeyChord, type ShortcutSection } from "../core/model";
import {
  core,
  type GlobalShortcutId,
  type ListedShortcut,
  type SectionOfShortcuts,
} from "../core/snappyCore";
import { ShortcutRecorder } from "../recorder/ShortcutRecorder";
import { ResetShortcutsButton } from "../shortcuts/ResetShortcutsButton";
import { useAppState, useSend } from "../state/services";
import { Group, Row } from "../ui/Group";
import { ArrowIcon } from "../ui/icons";

/** What a section says under its title, and more behind its ⓘ. */
function sectionCopy(section: ShortcutSection): { caption: string; info: string } {
  switch (section) {
    case "moveBetweenZones":
      return {
        caption: "Move the focused window to a neighboring or empty zone.",
        info: "With windows selected, the nearest empty zone shortcut moves them all.",
      };
    case "focusAndSelection":
      return {
        caption: "Move focus between windows, and select windows to move together.",
        info: "Selected windows go along when you switch or send to a workspace.",
      };
    case "workspaceSteps":
      return {
        caption: "Go to the previous or next workspace, or send windows there.",
        info: "Sending moves the focused or selected windows, and you stay where you are.",
      };
    case "windowHistory":
      return {
        caption: "Undo or redo the last window move.",
        info: `Snappy remembers your last ${coreLimits().undoSteps} moves. Neither has a shortcut until you record one.`,
      };
  }
}

/** The command that sets a global shortcut: the daemon takes each section's by its own. */
function setShortcut(id: GlobalShortcutId, chord: KeyChord | null): Command {
  switch (id.kind) {
    case "navigation":
      return { kind: "setNavigationShortcut", shortcut: id.shortcut, chord };
    case "workspaceStep":
      return { kind: "setWorkspaceStepShortcut", step: id.step, chord };
    case "history":
      return { kind: "setHistoryShortcut", step: id.step, chord };
  }
}

/** The direction a shortcut moves the focused window, which its row draws as an arrow. */
function movesToward(id: GlobalShortcutId): Direction | undefined {
  return id.kind === "navigation" ? DIRECTIONS.find((direction) => direction === id.shortcut) : undefined;
}

/** Every section of global shortcuts, in the order `snappy-core` gives them. */
export function ShortcutGroups() {
  const { settings } = useAppState();
  return (
    <>
      {core.shortcutSections(settings).map((section) => (
        <ShortcutSectionGroup key={section.section} section={section} />
      ))}
    </>
  );
}

function ShortcutSectionGroup({ section }: { section: SectionOfShortcuts }) {
  const { caption, info } = sectionCopy(section.section);
  return (
    <Group
      title={section.title}
      caption={caption}
      info={info}
      action={
        <ResetShortcutsButton
          title={section.title}
          reset={{ scope: "shortcuts", section: section.section }}
        />
      }
    >
      {section.shortcuts.map((shortcut) => (
        <ShortcutRow key={shortcut.title} shortcut={shortcut} />
      ))}
    </Group>
  );
}

/**
 * A row recording one shortcut, with an arrow before its title when it moves a window that way,
 * and a hint under it when its title doesn't say it all.
 */
function ShortcutRow({ shortcut }: { shortcut: ListedShortcut }) {
  const { settings, platform } = useAppState();
  const send = useSend();
  const hintId = useId();
  const direction = movesToward(shortcut.id);
  return (
    <Row
      label={
        <span className={direction === undefined ? "muted" : "direction-label muted"}>
          {direction !== undefined && <ArrowIcon direction={direction} />}
          {shortcut.title}
        </span>
      }
      hint={shortcut.hint}
      hintId={hintId}
    >
      <ShortcutRecorder
        shortcut={shortcut.id}
        title={shortcut.title}
        chord={shortcut.chord}
        conflictOf={(chord) => core.recordingConflict(settings, shortcut.id, chord, platform)}
        onChange={(chord) => void send(setShortcut(shortcut.id, chord))}
        describedBy={shortcut.hint === undefined ? undefined : hintId}
      />
    </Row>
  );
}
