import type { Display, Layout, LayoutOptions, Settings } from "./model";
import { core } from "./snappyCore";

let presets: Layout[] | undefined;

/** The eight presets, in their fixed order. */
export function presetLayouts(): Layout[] {
  presets ??= core.presets();
  return presets;
}

export function isPreset(layoutId: string): boolean {
  return presetLayouts().some((layout) => layout.id === layoutId);
}

/** Presets first, then custom layouts. */
export function allLayouts(settings: Settings): Layout[] {
  return [...presetLayouts(), ...settings.layouts.custom];
}

export function findLayout(settings: Settings, layoutId: string): Layout | undefined {
  return allLayouts(settings).find((layout) => layout.id === layoutId);
}

/** Stored options, or the defaults for a layout that has none. */
export function optionsFor(settings: Settings, layout: Layout): LayoutOptions {
  return settings.layouts.options[layout.id] ?? core.layoutOptions(settings, layout);
}

export function isEnabled(settings: Settings, layout: Layout): boolean {
  return optionsFor(settings, layout).enabled;
}

/** The layout a display uses right now. */
export function currentLayout(settings: Settings, displayId: string): Layout {
  return core.layoutForDisplay(settings, displayId);
}

/** Width over height of a display's work area, the shape previews and the canvas take. */
export function aspectRatio(display: Display | undefined): number {
  if (!display || display.workArea.height <= 0) {
    return 16 / 10;
  }
  return display.workArea.width / display.workArea.height;
}

/** The work area in backing pixels, the units of the editor's coordinate fields. */
export function pixelSize(display: Display | undefined): { width: number; height: number } {
  if (!display) {
    return { width: 0, height: 0 };
  }
  return {
    width: Math.round(display.workArea.width * display.scale),
    height: Math.round(display.workArea.height * display.scale),
  };
}
