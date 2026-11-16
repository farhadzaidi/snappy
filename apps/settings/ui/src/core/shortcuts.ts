import type { ShortcutId } from "./model";

/**
 * One string per shortcut, for finding its recorder on the page: its kind, then what picks it
 * out, each part escaped so no workspace, layout, or zone ID can make two shortcuts read alike.
 */
export function shortcutKey(id: ShortcutId): string {
  return [id.kind, ...identifying(id)].map(encodeURIComponent).join("/");
}

function identifying(id: ShortcutId): string[] {
  switch (id.kind) {
    case "navigation":
      return [id.shortcut];
    case "history":
    case "workspaceStep":
      return [id.step];
    case "workspace":
      return [id.workspace, id.shortcut];
    case "layoutSelect":
      return [id.layout];
    case "zone":
      return [id.layout, id.zone];
  }
}
