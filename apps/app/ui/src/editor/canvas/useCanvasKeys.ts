// The clipboard, Duplicate, Delete, and Merge keys, which act on the selection while the canvas has focus.
// Undo and redo work anywhere on the editor page; `EditorPage` handles them.

import type { KeyboardEvent } from "react";

import { usePlatform } from "../../state/services";
import type { EditorStore } from "../editorStore";
import { editorCommand, type EditorCommand } from "../keyboard";

export function useCanvasKeys(editor: EditorStore): (event: KeyboardEvent<HTMLElement>) => void {
  const platform = usePlatform();
  return (event) => {
    const action = canvasAction(editor, editorCommand(event, platform));
    if (action) {
      event.preventDefault();
      action();
    }
  };
}

/** What a command does on the canvas. A preset can't change, but its zones can be copied. */
function canvasAction(editor: EditorStore, command: EditorCommand | null): (() => void) | null {
  if (command === "copy") {
    return () => editor.copySelection();
  }
  if (editor.isReadOnly) {
    return null;
  }
  switch (command) {
    case "cut":
      return () => editor.cutSelection();
    case "paste":
      return () => editor.paste();
    case "duplicate":
      return () => editor.duplicateSelection();
    case "delete":
      return () => editor.deleteSelection();
    case "merge":
      return () => editor.mergeSelection();
    default:
      return null;
  }
}
