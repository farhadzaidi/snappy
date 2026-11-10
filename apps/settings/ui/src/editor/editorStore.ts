// The editor's state for the life of the settings app, and every action that
// changes it. Geometry comes from `snappy-core`; this file only records the results.

import { findLayout, isPreset, optionsFor } from "../core/layouts";
import type { KeyChord, Layout, LayoutOptions, Settings, UnitRect, Zone } from "../core/model";
import { core, type Axis, type CoordinateField } from "../core/snappyCore";
import type { DaemonClient } from "../state/daemonClient";
import { Observable } from "../state/observable";
import type { Session } from "../state/session";
import {
  beginStep,
  blankDraft,
  canAddZone,
  duplicateDraft,
  edit,
  endStep,
  followStored,
  hasChanges,
  keepSelection,
  markSaved,
  newId,
  openDraft,
  preview,
  redo,
  undo,
  type Draft,
  type SidebarTab,
  type Snapshot,
  type Tool,
} from "./draft";

function withZones(snapshot: Snapshot, zones: Zone[]): Snapshot {
  return { ...snapshot, layout: { ...snapshot.layout, zones } };
}

function withOptions(snapshot: Snapshot, change: Partial<LayoutOptions>): Snapshot {
  return { ...snapshot, options: { ...snapshot.options, ...change } };
}

export class EditorStore {
  readonly draft = new Observable<Draft>(blankDraft());
  /** "Start a new layout?" is showing. */
  readonly confirmingNew = new Observable<boolean>(false);
  /** A coordinate field holds an uncommitted different value. */
  readonly fieldPending = new Observable<boolean>(false);
  private commitField: (() => void) | null = null;

  constructor(
    private readonly client: DaemonClient,
    private readonly session: Observable<Session>,
  ) {}

  get isReadOnly(): boolean {
    return isPreset(this.draft.get().current.layout.id);
  }

  // Lifecycle

  /** "New": ask first when there are unsaved changes. */
  requestNew(): void {
    this.commitPendingField();
    this.session.update((session) => ({ ...session, page: "editor" }));
    if (hasChanges(this.draft.get())) {
      this.confirmingNew.set(true);
    } else {
      this.startNew();
    }
  }

  startNew(): void {
    this.confirmingNew.set(false);
    this.draft.set(blankDraft());
  }

  open(layout: Layout, options: LayoutOptions): void {
    this.draft.set(openDraft(layout, options));
    this.session.update((session) => ({ ...session, page: "editor" }));
  }

  duplicate(layout: Layout, options: LayoutOptions): void {
    this.draft.set(duplicateDraft(layout, options));
    this.session.update((session) => ({ ...session, page: "editor" }));
  }

  /** Keep the saved version in step with settings changed elsewhere. */
  follow(settings: Settings): void {
    const saved = this.draft.get().saved;
    const layout = saved && findLayout(settings, saved.layout.id);
    const stored = layout ? { layout, options: optionsFor(settings, layout) } : null;
    this.draft.update((draft) => followStored(draft, stored));
  }

  /** Save, apply to the editor's display, and browse the layout in the library. */
  async save(displayId: string): Promise<void> {
    this.commitPendingField();
    this.draft.update(endStep);
    const snapshot = this.draft.get().current;
    const error = await this.client.send({
      kind: "saveLayout",
      layout: snapshot.layout,
      options: snapshot.options,
      display: displayId,
    });
    if (error !== null) {
      return;
    }
    this.draft.update((draft) => markSaved(draft, snapshot));
    this.session.update((session) => ({
      ...session,
      collection: isPreset(snapshot.layout.id) ? "presets" : "custom",
      browsedLayoutId: snapshot.layout.id,
    }));
  }

  // Coordinate fields

  /** The focused coordinate field registers how to commit its text, so Save can. */
  setPendingField(commit: (() => void) | null, pending: boolean): void {
    this.commitField = commit;
    this.fieldPending.set(pending);
  }

  commitPendingField(): void {
    const commit = this.commitField;
    this.commitField = null;
    commit?.();
    this.fieldPending.set(false);
  }

  /** Apply a committed coordinate and return the zones that actually resulted. */
  setField(zoneId: string, field: CoordinateField, value: number): Zone[] {
    this.apply((snapshot) =>
      withZones(snapshot, core.setField({ zones: snapshot.layout.zones, id: zoneId, field, value })),
    );
    return this.draft.get().current.layout.zones;
  }

  // Selection and tools

  select(zoneId: string | null): void {
    this.draft.update((draft) => ({ ...draft, selectedZoneId: zoneId }));
  }

  setTool(tool: Tool): void {
    this.draft.update((draft) => ({ ...endStep(draft), tool }));
  }

  setTab(tab: SidebarTab): void {
    this.draft.update((draft) => ({ ...draft, tab }));
  }

  // Edits, each one undo step

  private apply(change: (snapshot: Snapshot) => Snapshot): void {
    this.draft.update((draft) => edit(draft, change));
  }

  private addZone(rect: UnitRect | null): void {
    if (!rect || !canAddZone(this.draft.get())) {
      return;
    }
    const zone = { id: newId(), rect };
    this.apply((snapshot) => withZones(snapshot, [...snapshot.layout.zones, zone]));
    this.select(zone.id);
  }

  /** Duplicate: a same-size copy in the nearest free spot, with no shortcut. */
  duplicateZone(zoneId: string): void {
    this.addZone(core.duplicateRect(this.draft.get().current.layout.zones, zoneId));
  }

  /** A zone drawn on empty canvas. */
  addDrawn(rect: UnitRect): void {
    this.addZone(rect);
  }

  /** Delete: remove the zone and its shortcut, then select the first zone. */
  deleteZone(zoneId: string): void {
    this.apply((snapshot) => {
      const { [zoneId]: _removed, ...zoneShortcuts } = snapshot.options.zoneShortcuts;
      const zones = snapshot.layout.zones.filter((zone) => zone.id !== zoneId);
      return withOptions(withZones(snapshot, zones), { zoneShortcuts });
    });
    this.draft.update((draft) => ({ ...draft, selectedZoneId: draft.current.layout.zones[0]?.id ?? null }));
  }

  /** Cut: the second part gets a new ID and is selected. */
  cut(zoneId: string, axis: Axis, at: number, tolerance: number): void {
    const newId_ = newId();
    const zones = core.cutZone({
      zones: this.draft.get().current.layout.zones,
      id: zoneId,
      axis,
      at,
      tolerance,
      newId: newId_,
    });
    if (!zones || !canAddZone(this.draft.get())) {
      return;
    }
    this.apply((snapshot) => withZones(snapshot, zones));
    this.select(newId_);
  }

  /** Align edges: coalesce nearby edges as one undo step. */
  alignEdges(): void {
    const aligned = core.alignEdges(this.draft.get().current.layout.zones);
    if (aligned) {
      this.apply((snapshot) => withZones(snapshot, aligned));
    }
  }

  setEnabled(enabled: boolean): void {
    this.apply((snapshot) => withOptions(snapshot, { enabled }));
  }

  setSelectShortcut(chord: KeyChord | null): void {
    this.apply((snapshot) => withOptions(snapshot, { selectShortcut: chord }));
  }

  setZoneShortcut(zoneId: string, chord: KeyChord | null): void {
    this.apply((snapshot) => {
      const { [zoneId]: _previous, ...others } = snapshot.options.zoneShortcuts;
      return withOptions(snapshot, { zoneShortcuts: chord ? { ...others, [zoneId]: chord } : others });
    });
  }

  clearZoneShortcuts(): void {
    this.apply((snapshot) => withOptions(snapshot, { zoneShortcuts: {} }));
  }

  // Steps that span many updates: canvas gestures and name editing

  beginStep(): void {
    this.draft.update(beginStep);
  }

  previewZones(zones: Zone[]): void {
    this.draft.update((draft) => preview(draft, (snapshot) => withZones(snapshot, zones)));
  }

  previewName(name: string): void {
    this.draft.update((draft) =>
      preview(draft, (snapshot) => ({ ...snapshot, layout: { ...snapshot.layout, name } })),
    );
  }

  endStep(): void {
    this.draft.update(endStep);
  }

  undo(): void {
    this.draft.update(undo);
  }

  redo(): void {
    this.draft.update((draft) => keepSelection(redo(draft)));
  }
}
