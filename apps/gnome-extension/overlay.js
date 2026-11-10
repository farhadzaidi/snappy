// The zone overlay: one container per display, drawn above every window, that never takes
// input or focus, and a second per display outlining marked windows, which stays while
// zones come and go. The app sends finished shapes, already placed and colored, so they
// look the same as on Snappy's other platforms.

import Clutter from 'gi://Clutter';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

/** A text's box is this many times its font size tall, room for the shadow too. */
const TEXT_BOX_PER_SIZE = 2;

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
    const height = text.size * TEXT_BOX_PER_SIZE;
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

function outlineActor(outline) {
    return new St.Widget({
        reactive: false,
        x: outline.x,
        y: outline.y,
        width: outline.width,
        height: outline.height,
        style:
            `border: ${cssPixels(outline.borderWidth)} solid ${rgba(outline.border)}; ` +
            `border-radius: ${cssPixels(outline.radius)};`,
    });
}

export class Overlays {
    constructor() {
        /** Display ID to its container and the scene it shows. */
        this._displays = new Map();
        this._visible = null;
        /** Display ID to the container outlining its marked windows, apart from the zones. */
        this._marks = new Map();
        this._locked = false;
    }

    /** Outlines a display's marked windows until the next call for it; none removes them. */
    mark(display, marks) {
        let actor = this._marks.get(display);
        if (!actor) {
            if (marks.outlines.length === 0)
                return;
            actor = new St.Widget({name: 'snappy-overlay', reactive: false, visible: false});
            Main.layoutManager.uiGroup.add_child(actor);
            this._marks.set(display, actor);
        }
        actor.set_position(marks.workArea.x, marks.workArea.y);
        actor.set_size(marks.workArea.width, marks.workArea.height);
        actor.destroy_all_children();
        for (const outline of marks.outlines)
            actor.add_child(outlineActor(outline));
        Main.layoutManager.uiGroup.set_child_above_sibling(actor, null);
        actor.visible = marks.outlines.length > 0 && !this._locked;
    }

    /** Nothing of Snappy's shows on the lock screen; marks come back when it goes. */
    setLocked(locked) {
        this._locked = locked;
        if (locked)
            this.hide();
        for (const actor of this._marks.values())
            actor.visible = !locked && actor.get_n_children() > 0;
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

    /** Drops the overlays and marks of displays that are gone. */
    retain(displays) {
        for (const [display, entry] of this._displays) {
            if (displays.includes(display))
                continue;
            entry.actor.destroy();
            this._displays.delete(display);
            if (this._visible === display)
                this._visible = null;
        }
        for (const [display, actor] of this._marks) {
            if (displays.includes(display))
                continue;
            actor.destroy();
            this._marks.delete(display);
        }
    }

    /** Takes every overlay down. */
    clear() {
        this.retain([]);
    }
}
