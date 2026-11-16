/**
 * The parts of a key press that shortcuts look at, as `KeyboardEvent` has them, so events pass
 * as they are. `code` is the physical key and `key` the character it produced.
 */
export type KeyPress = Pick<KeyboardEvent, "code" | "key" | "shiftKey" | "ctrlKey" | "altKey" | "metaKey">;

/** Delete (Backspace) and Forward Delete, which remove what is selected or recorded. */
export const DELETE_KEY = "Backspace";
const DELETING_CODES: ReadonlySet<string> = new Set([DELETE_KEY, "Delete"]);

export function isDeleting(press: Pick<KeyPress, "code">): boolean {
  return DELETING_CODES.has(press.code);
}
