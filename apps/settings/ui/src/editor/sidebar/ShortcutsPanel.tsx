import type { LayoutOptions } from "../../core/model";
import { core } from "../../core/snappyCore";
import { ShortcutRecorder } from "../../recorder/ShortcutRecorder";
import { useObservable } from "../../state/observable";
import { useAppState, useServices } from "../../state/services";
import { draftConflict } from "../saving";

/** How the shortcuts' titles name the layout they belong to, which the editor shows. */
const THIS_LAYOUT = "this layout";

/** The layout's selection shortcut and one recorder per zone. */
export function ShortcutsPanel() {
  const app = useAppState();
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const { layout, options } = draft.current;
  const titles = core.layoutShortcutTitles(THIS_LAYOUT, layout.zones.length);

  /** The conflict the draft would have with `change` applied. */
  const conflictWith = (change: Partial<LayoutOptions>) =>
    draftConflict({ layout, options: { ...options, ...change } }, app.settings, app.platform);

  return (
    <div className="shortcuts-panel">
      <h2 className="panel-title">{titles.select}</h2>
      <ShortcutRecorder
        title={titles.select}
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
              title={titles.zones[index]}
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
