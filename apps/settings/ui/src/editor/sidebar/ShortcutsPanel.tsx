import type { LayoutOptions } from "../../core/model";
import { ShortcutRecorder } from "../../recorder/ShortcutRecorder";
import { useObservable } from "../../state/observable";
import { useAppState, useServices } from "../../state/services";
import { draftConflict } from "../saving";

/** The layout's selection shortcut and one recorder per zone. */
export function ShortcutsPanel() {
  const app = useAppState();
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const { layout, options } = draft.current;

  /** The conflict the draft would have with `change` applied. */
  const conflictWith = (change: Partial<LayoutOptions>) =>
    draftConflict({ layout, options: { ...options, ...change } }, app.settings, app.platform);

  return (
    <div className="shortcuts-panel">
      <h2 className="panel-title">Select this layout</h2>
      <ShortcutRecorder
        title="Select layout shortcut"
        chord={options.selectShortcut}
        conflictOf={(chord) => conflictWith({ selectShortcut: chord })}
        onChange={(chord) => editor.setSelectShortcut(chord)}
      />
      <hr className="sidebar-divider" />
      <div className="panel-heading">
        <h2 className="panel-title">Zone shortcuts</h2>
        <button type="button" className="text-button" onClick={() => editor.clearZoneShortcuts()}>
          Clear all
        </button>
      </div>
      <ul className="zone-shortcuts">
        {layout.zones.map((zone, index) => (
          <li key={zone.id} className="zone-shortcut">
            <button
              type="button"
              className="zone-shortcut-number"
              aria-label={`Select zone ${index + 1}`}
              data-selected={zone.id === draft.selectedZoneId}
              onClick={() => editor.select(zone.id)}
            >
              {index + 1}
            </button>
            <ShortcutRecorder
              title={`Zone ${index + 1} shortcut`}
              chord={options.zoneShortcuts[zone.id] ?? null}
              conflictOf={(chord) =>
                conflictWith({ zoneShortcuts: { ...options.zoneShortcuts, [zone.id]: chord } })
              }
              onChange={(chord) => editor.setZoneShortcut(zone.id, chord)}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}
