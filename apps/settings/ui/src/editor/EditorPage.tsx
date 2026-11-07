import { useEffect } from "react";

import { useObservable } from "../state/observable";
import { usePlatform, useServices } from "../state/services";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { UndoIcon } from "../ui/icons";
import { Canvas } from "./canvas/Canvas";
import { Sidebar } from "./sidebar/Sidebar";
import { TitleBar } from "./TitleBar";
import { Toolbar } from "./Toolbar";

/** The layout editor. */
export function EditorPage() {
  const { editor } = useServices();
  const confirmingNew = useObservable(editor.confirmingNew);
  useUndoShortcuts();

  return (
    <div className="editor-page">
      <div className="editor-main">
        <TitleBar />
        {!editor.isReadOnly && <Toolbar />}
        <Canvas />
      </div>
      <Sidebar />
      {confirmingNew && (
        <ConfirmDialog
          title="Start a new layout?"
          message="Your unsaved changes will be discarded. Saved layouts will be kept."
          confirmTitle="Discard changes"
          cancelTitle="Keep editing"
          icon={<UndoIcon />}
          onConfirm={() => editor.startNew()}
          onCancel={() => editor.confirmingNew.set(false)}
        />
      )}
    </div>
  );
}

function isTextField(target: EventTarget | null): boolean {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
}

/**
 * Command-Z and Shift-Command-Z (Control on Windows and Linux) undo and redo whole
 * edits. Inside a text field they act on the field's text instead.
 */
function useUndoShortcuts() {
  const { editor } = useServices();
  const platform = usePlatform();
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const command = platform === "macos" ? event.metaKey : event.ctrlKey;
      if (!command || event.code !== "KeyZ" || isTextField(event.target)) {
        return;
      }
      event.preventDefault();
      editor.commitPendingField();
      if (event.shiftKey) {
        editor.redo();
      } else {
        editor.undo();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [editor, platform]);
}
