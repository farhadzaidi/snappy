import type { ReactNode } from "react";

import { useObservable } from "../state/observable";
import { useServices } from "../state/services";
import { IconButton } from "../ui/Button";
import { CursorIcon, RedoIcon, SplitHorizontalIcon, SplitVerticalIcon, UndoIcon } from "../ui/icons";
import type { Tool } from "./draft";

const TOOLS: readonly { tool: Tool; title: string; help: string; icon: ReactNode }[] = [
  {
    tool: "select",
    title: "Select",
    help: "Drag a zone to move it, an edge to resize, or empty space to draw.",
    icon: <CursorIcon />,
  },
  {
    tool: "verticalCut",
    title: "Vertical cut",
    help: "Click a zone to cut it vertically at the pointer.",
    icon: <SplitVerticalIcon />,
  },
  {
    tool: "horizontalCut",
    title: "Horizontal cut",
    help: "Click a zone to cut it horizontally at the pointer.",
    icon: <SplitHorizontalIcon />,
  },
];

/** The tools, then Undo and Redo. */
export function Toolbar() {
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const run = (action: () => void) => () => {
    editor.commitPendingField();
    action();
  };

  return (
    <div className="editor-toolbar" role="toolbar" aria-label="Editing tools">
      <div className="segmented tool-group">
        {TOOLS.map(({ tool, title, help, icon }) => (
          <button
            type="button"
            key={tool}
            className="segment tool"
            aria-pressed={draft.tool === tool}
            title={help}
            onClick={run(() => editor.setTool(tool))}
          >
            {icon}
            {title}
          </button>
        ))}
      </div>
      <div className="history-buttons">
        <IconButton
          label="Undo edit"
          disabled={draft.past.length === 0 && !draft.stepStart}
          onClick={run(() => editor.undo())}
        >
          <UndoIcon />
        </IconButton>
        <IconButton label="Redo edit" disabled={draft.future.length === 0} onClick={run(() => editor.redo())}>
          <RedoIcon />
        </IconButton>
      </div>
    </div>
  );
}
