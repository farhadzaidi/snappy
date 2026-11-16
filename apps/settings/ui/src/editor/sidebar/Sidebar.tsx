import { core } from "../../core/snappyCore";
import { DisplayChooser } from "../../shell/DisplayChooser";
import { useObservable } from "../../state/observable";
import { useAppState, useServices } from "../../state/services";
import { Button } from "../../ui/Button";
import { Toggle } from "../../ui/Toggle";
import { WarningNotice } from "../../ui/WarningNotice";
import type { SidebarTab } from "../draft";
import { draftConflict } from "../saving";
import { ShortcutsPanel } from "./ShortcutsPanel";
import { ZoneInspector } from "./ZoneInspector";

const TABS: readonly { tab: SidebarTab; title: string }[] = [
  { tab: "zone", title: "Zone" },
  { tab: "shortcuts", title: "Shortcuts" },
];

/** The display, "Layout enabled", and the Zone and Shortcuts tabs. */
export function Sidebar() {
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const readOnly = editor.isReadOnly;
  const showShortcuts = readOnly || draft.tab === "shortcuts";

  return (
    <aside className="editor-sidebar" aria-label="Layout settings">
      <DisplayChooser />
      <Toggle
        label="Layout enabled"
        checked={draft.current.options.enabled}
        title="Disable to keep this layout and its shortcuts saved without registering them."
        onChange={(enabled) => editor.setEnabled(enabled)}
      />
      <hr className="sidebar-divider" />
      {!readOnly && (
        <div className="segmented segmented-even sidebar-tabs" role="tablist" aria-label="Sidebar">
          {TABS.map(({ tab, title }) => (
            <button
              type="button"
              role="tab"
              key={tab}
              className="segment"
              aria-selected={draft.tab === tab}
              onClick={() => {
                editor.commitPendingField();
                editor.setTab(tab);
              }}
            >
              {title}
            </button>
          ))}
        </div>
      )}
      {showShortcuts ? <ShortcutsPanel /> : <ZoneInspector />}
      <OverlapNotice />
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
