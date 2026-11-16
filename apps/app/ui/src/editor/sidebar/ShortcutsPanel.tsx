import type { LayoutOptions, Refusal } from "../../core/model";
import { core } from "../../core/snappyCore";
import { ShortcutRecorder } from "../../recorder/ShortcutRecorder";
import { useObservable } from "../../state/observable";
import { useAppState, useServices } from "../../state/services";
import { Button } from "../../ui/Button";
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

  /**
   * The conflict the draft would have with `change` applied. It names no shortcut to point at,
   * since the editor shows no other shortcuts.
   */
  const conflictWith = (change: Partial<LayoutOptions>): Refusal | null => {
    const message = draftConflict({ layout, options: { ...options, ...change } }, app.settings, app.platform);
    return message === null ? null : { message, shortcut: null };
  };

  return (
    <div className="shortcuts-panel">
      <h2 className="panel-title">{titles.select}</h2>
      <div className="layout-shortcut">
        <ShortcutRecorder
          shortcut={{ kind: "layoutSelect", layout: layout.id }}
          title={titles.select}
          chord={options.selectShortcut}
          conflictOf={(chord) => conflictWith({ selectShortcut: chord })}
          onChange={(chord) => editor.setSelectShortcut(chord)}
        />
      </div>
      <hr className="sidebar-divider" />
      <h2 className="panel-title">Zone shortcuts</h2>
      <ul className="zone-shortcuts">
        {layout.zones.map((zone, index) => (
          <li key={zone.id} className="zone-shortcut">
            <button
              type="button"
              className="zone-shortcut-number"
              aria-label={`Select zone ${index + 1}`}
              data-selected={draft.selection.includes(zone.id)}
              onClick={() => editor.select(zone.id)}
            >
              {index + 1}
            </button>
            <ShortcutRecorder
              shortcut={{ kind: "zone", layout: layout.id, zone: zone.id }}
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
      <Button
        className="clear-shortcuts"
        title="A layout's shortcuts start empty. Save to keep the change."
        disabled={options.selectShortcut === null && Object.keys(options.zoneShortcuts).length === 0}
        onClick={() => editor.clearShortcuts()}
      >
        Clear all
      </Button>
    </div>
  );
}
