import { core } from "../../core/snappyCore";
import { DisplayChooser } from "../../shell/DisplayChooser";
import { WarningNotice } from "../../shortcuts/WarningNotice";
import { useObservable } from "../../state/observable";
import { useAppState, useServices } from "../../state/services";
import { Button } from "../../ui/Button";
import { tabPanelProps, Tabs, useTabIds } from "../../ui/Tabs";
import { Toggle } from "../../ui/Toggle";
import type { SidebarTab } from "../draft";
import { draftConflict, nameProblem } from "../saving";
import { ShortcutsPanel } from "./ShortcutsPanel";
import { ZoneInspector } from "./ZoneInspector";

const TABS: readonly { value: SidebarTab; title: string }[] = [
  { value: "zone", title: "Zone" },
  { value: "shortcuts", title: "Shortcuts" },
];

/** The display, "Layout enabled", and the Zone and Shortcuts tabs. */
export function Sidebar() {
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const tabIds = useTabIds();
  const readOnly = editor.isReadOnly;
  const showShortcuts = draft.tab === "shortcuts";

  return (
    <aside className="editor-sidebar" aria-label="Layout settings">
      <DisplayChooser />
      <Toggle
        label="Layout enabled"
        checked={draft.current.options.enabled}
        title="Turn off to keep this layout without using it or its shortcuts."
        onChange={(enabled) => editor.setEnabled(enabled)}
      />
      <hr className="sidebar-divider" />
      {readOnly ? (
        <ShortcutsPanel />
      ) : (
        <>
          <Tabs
            label="Sidebar"
            className="segmented segmented-even sidebar-tabs"
            tabs={TABS}
            selected={draft.tab}
            ids={tabIds}
            onSelect={(tab) => {
              editor.commitPendingField();
              editor.setTab(tab);
            }}
          />
          <div {...tabPanelProps(tabIds, draft.tab)}>
            {showShortcuts ? <ShortcutsPanel /> : <ZoneInspector />}
          </div>
        </>
      )}
      <OverlapNotice />
      <NameNotice />
      <ConflictNotice />
    </aside>
  );
}

/** Overlapping drafts can be repaired when nearby edges line up cleanly. */
function OverlapNotice() {
  const { editor } = useServices();
  const zones = useObservable(editor.draft).current.layout.zones;
  if (core.isSeparated(zones)) {
    return null;
  }
  const repairable = core.alignEdges(zones) !== null;
  return (
    <div className="notice">
      <p className="muted">This draft has overlapping edges.</p>
      {repairable ? (
        <Button title="Align nearby edges without deleting zones." onClick={() => editor.alignEdges()}>
          Align edges
        </Button>
      ) : (
        <p className="muted">Move the overlapping zones apart before saving.</p>
      )}
    </div>
  );
}

/** A blank or over-long name blocks saving, so say so beside the other reasons. */
function NameNotice() {
  const { editor } = useServices();
  const problem = nameProblem(useObservable(editor.draft).current);
  if (problem === null) {
    return null;
  }
  return (
    <div className="notice">
      <p className="muted">{problem}</p>
    </div>
  );
}

/** A shortcut conflict shows here and blocks saving. */
function ConflictNotice() {
  const app = useAppState();
  const { editor } = useServices();
  const conflict = draftConflict(useObservable(editor.draft).current, app.settings, app.platform);
  if (conflict === null) {
    return null;
  }
  return <WarningNotice message={conflict} />;
}
