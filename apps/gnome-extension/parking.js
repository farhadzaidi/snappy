// Hiding windows for Snappy's workspaces: a GNOME workspace of Snappy's own holds them,
// with their frames left alone. With dynamic workspaces GNOME keeps an empty workspace last,
// so the parking workspace stays just before it; with a fixed number it is added last and
// removed again when Snappy's workspaces stop. Parked windows are left out of the window
// switcher and the overview, so they stay out of sight.

import Meta from 'gi://Meta';

import {InjectionManager} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Workspace from 'resource:///org/gnome/shell/ui/workspace.js';
import * as WorkspaceThumbnail from 'resource:///org/gnome/shell/ui/workspaceThumbnail.js';

/** Why a window can't be parked, as the app's protocol names it. */
export const REFUSED_ALL_WORKSPACES = 'allWorkspaces';

export class Parking {
    constructor(emit) {
        this._emit = emit;
        /** The workspace Snappy's workspaces belong to. */
        this._bound = null;
        this._parking = null;
        this._injections = new InjectionManager();
        this._hideParkedWindows();
        global.workspace_manager.connectObject(
            'workspace-removed', () => this._forgetRemoved(),
            'workspace-added', () => this._keepInPlace(),
            'active-workspace-changed', () => this._emit({type: 'workspace'}),
            this);
    }

    _hideParkedWindows() {
        const parking = this;
        this._injections.overrideMethod(Meta.Display.prototype, 'get_tab_list', original =>
            function (...args) {
                return original.call(this, ...args).filter(window => !parking.isParked(window));
            });
        for (const prototype of [Workspace.Workspace?.prototype, WorkspaceThumbnail.WorkspaceThumbnail?.prototype]) {
            if (typeof prototype?._isOverviewWindow !== 'function')
                continue;
            this._injections.overrideMethod(prototype, '_isOverviewWindow', original =>
                function (window) {
                    const metaWindow = window.meta_window ?? window;
                    return !parking.isParked(metaWindow) && original.call(this, window);
                });
        }
    }

    _workspaces() {
        const manager = global.workspace_manager;
        return Array.from({length: manager.get_n_workspaces()}, (_, index) => manager.get_workspace_by_index(index));
    }

    _exists(workspace) {
        return workspace !== null && this._workspaces().includes(workspace);
    }

    /**
     * Where shown windows belong: the bound workspace, else the one in use, else the first.
     * Never the parking workspace, which the user may be looking at.
     */
    _home() {
        const candidates = [this._bound, global.workspace_manager.get_active_workspace(), ...this._workspaces()];
        return candidates.find(workspace => this._exists(workspace) && workspace !== this._parking) ?? null;
    }

    _forgetRemoved() {
        let changed = false;
        if (this._parking !== null && !this._exists(this._parking)) {
            // GNOME moved what it held onto a visible workspace; the app hears it and looks.
            this._parking = null;
            changed = true;
        }
        if (this._bound !== null && !this._exists(this._bound)) {
            // GNOME removes an empty workspace the user left; the one shown windows go to
            // next takes its place, as a desktop number does on X11.
            this._bound = this._home();
            changed = true;
        }
        if (changed)
            this._emit({type: 'workspace'});
    }

    /** Dynamic workspaces: just before GNOME's empty last one. Fixed: last. */
    _keepInPlace() {
        if (!this._exists(this._parking))
            return;
        const manager = global.workspace_manager;
        const count = manager.get_n_workspaces();
        const wanted = Meta.prefs_get_dynamic_workspaces() ? count - 2 : count - 1;
        if (wanted >= 0 && this._parking.index() !== wanted)
            manager.reorder_workspace(this._parking, wanted);
    }

    _ensureParking() {
        if (!this._exists(this._parking)) {
            this._parking = global.workspace_manager.append_new_workspace(false, global.get_current_time());
            this._keepInPlace();
        }
        return this._parking;
    }

    isParked(window) {
        return this._parking !== null && window.get_workspace() === this._parking &&
            !window.is_on_all_workspaces();
    }

    /** Binds Snappy's workspaces to the workspace in use now. */
    bind() {
        this._bound = global.workspace_manager.get_active_workspace();
    }

    onBoundWorkspace() {
        return this._bound !== null && global.workspace_manager.get_active_workspace() === this._bound;
    }

    /** Hides a window on the parking workspace. Returns a refusal, or null. */
    park(window) {
        if (window.is_on_all_workspaces())
            return REFUSED_ALL_WORKSPACES;
        const parking = this._ensureParking();
        if (window.get_workspace() !== parking)
            window.change_workspace(parking);
        if (window.is_on_all_workspaces() || window.get_workspace() !== parking)
            return REFUSED_ALL_WORKSPACES;
        return null;
    }

    /** Brings a window back to where shown windows belong. */
    unpark(window) {
        const home = this._home();
        if (home !== null && window.get_workspace() !== home && !window.is_on_all_workspaces())
            window.change_workspace(home);
    }

    /**
     * Stops: unbinds, and removes the parking workspace once nothing is on it. Someone looking
     * at the empty parking workspace is taken to where shown windows belong first, since GNOME
     * won't remove the workspace in use. Returns whether it is gone.
     */
    clear() {
        const home = this._home();
        this._bound = null;
        if (!this._exists(this._parking)) {
            this._parking = null;
            return true;
        }
        if (this._parking.list_windows().some(window => !window.is_on_all_workspaces()))
            return false;
        const manager = global.workspace_manager;
        if (manager.get_active_workspace() === this._parking) {
            if (home === null)
                return false;
            home.activate(global.get_current_time());
        }
        manager.remove_workspace(this._parking, global.get_current_time());
        this._parking = null;
        return true;
    }

    /** Every parked window goes back, frames untouched, so none is lost while turned off. */
    destroy() {
        if (this._exists(this._parking)) {
            for (const window of this._parking.list_windows()) {
                if (!window.is_on_all_workspaces())
                    this.unpark(window);
            }
        }
        this.clear();
        global.workspace_manager.disconnectObject(this);
        this._injections.clear();
    }
}
