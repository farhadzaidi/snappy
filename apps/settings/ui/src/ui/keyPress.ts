/**
 * The parts of a key press that shortcuts look at, as `KeyboardEvent` has them, so events pass
 * as they are. `code` is the physical key and `key` the character it produced.
 */
export type KeyPress = Pick<KeyboardEvent, "code" | "key" | "shiftKey" | "ctrlKey" | "altKey" | "metaKey">;
