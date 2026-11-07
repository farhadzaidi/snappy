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
 * The browsed layout is the one last clicked; otherwise the selected display's layout when
 * it belongs to the collection, otherwise the collection's first.
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

export function layoutStatus(
  app: AppState,
  layout: Layout,
  inUse: Layout | undefined,
): "inUse" | "inactive" | null {
  if (!isEnabled(app.settings, layout)) {
    return "inactive";
  }
  return layout.id === inUse?.id ? "inUse" : null;
}
