// The zone overlay: one container per display, drawn above every window, that never takes
// input or focus. The app sends finished shapes, already placed and colored, so they look
// the same as on Snappy's other platforms.

import Clutter from 'gi://Clutter';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

/** CSS lengths in St grow with the UI scale; the app's sizes are already in stage pixels. */
function cssPixels(stagePixels) {
    const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor || 1;
    return `${(stagePixels / scale).toFixed(2)}px`;
}

function rgba([red, green, blue, alpha]) {
    return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

/** A text centered on `center` inside a box as wide as the zone. */
function centeredText(zone, text) {
    const height = text.size * 2;
    const label = new St.Label({
        text: text.text,
        style:
            `font-size: ${cssPixels(text.size)}; font-weight: bold; color: rgba(255, 255, 255, 1); ` +
            `text-shadow: 0 0 ${cssPixels(text.shadowBlur)} rgba(0, 0, 0, ${text.shadowOpacity});`,
    });
    return new St.Bin({
        reactive: false,
        x: zone.x,
        y: text.centerY - height / 2,
        width: zone.width,
        height,
        x_align: Clutter.ActorAlign.CENTER,
        y_align: Clutter.ActorAlign.CENTER,
        child: label,
    });
}

function zoneActor(zone) {
    return new St.Widget({
        reactive: false,
        x: zone.x,
        y: zone.y,
        width: zone.width,
        height: zone.height,
        style:
            `background-color: ${rgba(zone.fill)}; ` +
            `border: ${cssPixels(zone.borderWidth)} solid ${rgba(zone.border)}; ` +
            `border-radius: ${cssPixels(zone.radius)};`,
    });
}

export class Overlays {
    constructor() {
        /** Display ID to its container and the scene it shows. */
        this._displays = new Map();
        this._visible = null;
    }

    _container(display, workArea) {
        let entry = this._displays.get(display);
        if (!entry) {
            const actor = new St.Widget({name: 'snappy-overlay', reactive: false, visible: false});
            Main.layoutManager.uiGroup.add_child(actor);
            entry = {actor, scene: null};
            this._displays.set(display, entry);
        }
        entry.actor.set_position(workArea.x, workArea.y);
        entry.actor.set_size(workArea.width, workArea.height);
        return entry;
    }

    /** Builds a display's overlay without showing it. */
    prepare(display, scene) {
        const entry = this._container(display, scene.workArea);
        const drawn = JSON.stringify(scene);
        if (entry.scene === drawn)
            return entry;
        entry.actor.destroy_all_children();
        for (const zone of scene.zones) {
            entry.actor.add_child(zoneActor(zone));
            entry.actor.add_child(centeredText(zone, zone.number));
            if (zone.caption)
                entry.actor.add_child(centeredText(zone, zone.caption));
        }
        entry.scene = drawn;
        return entry;
    }

    /** Shows one display's overlay, hiding any other. */
    show(display, scene) {
        const entry = this.prepare(display, scene);
        if (this._visible !== display)
            this.hide();
        Main.layoutManager.uiGroup.set_child_above_sibling(entry.actor, null);
        entry.actor.show();
        this._visible = display;
    }

    hide() {
        if (this._visible === null)
            return;
        this._displays.get(this._visible)?.actor.hide();
        this._visible = null;
    }

    /** Drops the overlays of displays that are gone. */
    retain(displays) {
        for (const [display, entry] of this._displays) {
            if (displays.includes(display))
                continue;
            entry.actor.destroy();
            this._displays.delete(display);
            if (this._visible === display)
                this._visible = null;
        }
    }

    destroy() {
        this.retain([]);
    }
}
