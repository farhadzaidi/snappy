import { currentLayout, isEnabled, presetLayouts } from "../core/layouts";
import type { AppState, Layout } from "../core/model";
import { selectedDisplay, type Collection, type Session } from "../state/session";

export interface Library {
  layouts: Layout[];
  /** The previewed layout, or undefined for an empty collection. */
  browsed: Layout | undefined;
  /** The selected display's layout. */
  inUse: Layout | undefined;
  displayId: string | undefined;
}

export function collectionLayouts(app: AppState, collection: Collection): Layout[] {
  return collection === "presets" ? presetLayouts() : app.settings.layouts.custom;
}

/**
 * The browsed layout is the one the page keeps (`browsedLayoutId`); until there is one, or
 * once it is gone, the selected display's layout when it belongs to the collection, otherwise
 * the collection's first. The page keeps whatever it shows, so changes to the display's
 * layout, such as disabling it, never browse another one.
 */
export function library(app: AppState, session: Session): Library {
  const display = selectedDisplay(app, session);
  const layouts = collectionLayouts(app, session.collection);
  const inUse = display ? currentLayout(app.settings, display.id) : undefined;
  const browsed =
    layouts.find((layout) => layout.id === session.browsedLayoutId) ??
    layouts.find((layout) => layout.id === inUse?.id) ??
    layouts[0];
  return { layouts, browsed, inUse, displayId: display?.id };
}

/** In use on the selected display, disabled, or neither. */
export type LayoutStatus = "inUse" | "disabled" | null;

export function layoutStatus(app: AppState, layout: Layout, inUse: Layout | undefined): LayoutStatus {
  if (!isEnabled(app.settings, layout)) {
    return "disabled";
  }
  return layout.id === inUse?.id ? "inUse" : null;
}
