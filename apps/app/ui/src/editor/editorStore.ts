// The editor's state for the life of the app, and every action that
// changes it. Geometry comes from `snappy-core`; this file only records the results.

import { findLayout, isPreset, optionsFor } from "../core/layouts";
import type { KeyChord, Layout, LayoutOptions, Settings, UnitRect, Zone } from "../core/model";
import {
  core,
  type Axis,
  type CoordinateField,
  type CutScope,
  type MergeOutcome,
  type PlaceOutcome,
} from "../core/snappyCore";
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
  firstZone,
  followStored,
  hasChanges,
  keepSelection,
  markSaved,
  newId,
  openDraft,
  preview,
  redo,
  selectedZones,
  toggled,
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
  /** The zone geometry Copy and Cut took, for Paste. It outlives drafts, so zones paste into other layouts. */
  private clipboard: UnitRect[] = [];

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

  /** Apply a committed coordinate; core keeps the zones sound, which may change others too. */
  setField(zoneId: string, field: CoordinateField, value: number): void {
    this.apply((snapshot) =>
      withZones(snapshot, core.setField({ zones: snapshot.layout.zones, id: zoneId, field, value })),
    );
  }

  // Selection and tools

  /** Select one zone, or none. */
  select(zoneId: string | null): void {
    this.setSelection(zoneId ? [zoneId] : []);
  }

  /** Shift-click: add a zone to the selection, or take it out. */
  toggleSelected(zoneId: string): void {
    this.setSelection(toggled(this.draft.get().selection, zoneId));
  }

  private setSelection(selection: string[]): void {
    this.draft.update((draft) => ({ ...draft, selection }));
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

  /** Add zones at `rects`, with no shortcuts, and select them. */
  private addZones(rects: UnitRect[]): void {
    const added = rects.map((rect) => ({ id: newId(), rect }));
    this.apply((snapshot) => withZones(snapshot, [...snapshot.layout.zones, ...added]));
    this.setSelection(added.map((zone) => zone.id));
  }

  /** A zone drawn on empty canvas. */
  addDrawn(rect: UnitRect): void {
    if (canAddZone(this.draft.get())) {
      this.addZones([rect]);
    }
  }

  private selectedRects(): UnitRect[] {
    return selectedZones(this.draft.get()).map((zone) => zone.rect);
  }

  /** Where copies of `rects` go, or why they can't go anywhere. */
  private placeCopies(rects: UnitRect[]): PlaceOutcome {
    return core.placeCopies(this.draft.get().current.layout.zones, rects);
  }

  /** Add the placed copies, or say why there are none, since a key press shows nothing else. */
  private addCopies(outcome: PlaceOutcome): void {
    if ("placed" in outcome) {
      this.addZones(outcome.placed);
    } else {
      this.client.announce(outcome.refused, true);
    }
  }

  /** Why the selected zones can't be duplicated, or null when they can. */
  duplicateRefusal(): string | null {
    const outcome = this.placeCopies(this.selectedRects());
    return "refused" in outcome ? outcome.refused : null;
  }

  /** Duplicate: same-size copies of the selected zones, placed as Paste places them. */
  duplicateSelection(): void {
    const rects = this.selectedRects();
    if (rects.length > 0) {
      this.addCopies(this.placeCopies(rects));
    }
  }

  /** Copy: remember the selected zones' geometry for Paste. Nothing changes, so it is no step. */
  copySelection(): void {
    const rects = this.selectedRects();
    if (rects.length > 0) {
      this.clipboard = rects;
    }
  }

  /** Cut: Copy, then Delete. */
  cutSelection(): void {
    this.copySelection();
    this.deleteSelection();
  }

  /** Paste: copies of the copied zones where `snappy-core` places them, selected. */
  paste(): void {
    this.addCopies(this.placeCopies(this.clipboard));
  }

  /** Delete: remove the selected zones and their shortcuts, then select the first zone. */
  deleteSelection(): void {
    const removed = new Set(this.draft.get().selection);
    if (removed.size === 0) {
      return;
    }
    this.apply((snapshot) => {
      const zones = snapshot.layout.zones.filter((zone) => !removed.has(zone.id));
      const zoneShortcuts = Object.fromEntries(
        Object.entries(snapshot.options.zoneShortcuts).filter(([id]) => !removed.has(id)),
      );
      return withOptions(withZones(snapshot, zones), { zoneShortcuts });
    });
    this.draft.update((draft) => ({ ...draft, selection: firstZone(draft.current) }));
  }

  /** The selected zones merged, or why they can't be. */
  private mergeOutcome(): MergeOutcome {
    const { current, selection } = this.draft.get();
    return core.mergeZones(current.layout.zones, selection, current.options.zoneShortcuts);
  }

  /** Why the selected zones can't be merged, or null when they can. */
  mergeRefusal(): string | null {
    const outcome = this.mergeOutcome();
    return "refused" in outcome ? outcome.refused : null;
  }

  /** Merge: the selected zones become one, which is selected. Shift-M without one says why. */
  mergeSelection(): void {
    const outcome = this.mergeOutcome();
    if (!("merged" in outcome)) {
      this.client.announce(outcome.refused, true);
      return;
    }
    const { zones, zoneShortcuts, kept } = outcome.merged;
    this.apply((snapshot) => withOptions(withZones(snapshot, zones), { zoneShortcuts }));
    this.select(kept);
  }

  /**
   * Cut at `at`, a line the cut preview already snapped. The second part of `zoneId`, the
   * zone clicked, is selected; a cut through the layout that leaves that zone whole keeps it.
   */
  cut(scope: CutScope, axis: Axis, at: number, zoneId: string): void {
    const zones = this.draft.get().current.layout.zones;
    const result = core.cutZones({ zones, scope, axis, at, newIds: zones.map(() => newId()) });
    if (!result) {
      return;
    }
    this.apply((snapshot) => withZones(snapshot, result));
    const after = result[result.findIndex((zone) => zone.id === zoneId) + 1];
    const isNew = after !== undefined && !zones.some((zone) => zone.id === after.id);
    this.select(isNew ? after.id : zoneId);
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

  /**
   * Every shortcut of the layout's own, its selection's and its zones', which have none by
   * default: one step, as any edit, saved with the layout.
   */
  clearShortcuts(): void {
    this.apply((snapshot) => withOptions(snapshot, { selectShortcut: null, zoneShortcuts: {} }));
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
