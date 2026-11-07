import { optionsFor } from "../core/layouts";
import { useObservable } from "../state/observable";
import { useAppState, useSelectedDisplay, useServices } from "../state/services";
import { Button } from "../ui/Button";
import { LockIcon } from "../ui/icons";
import { canSave } from "./saving";

/** The name above the canvas, and New, Duplicate, and Save. */
export function TitleBar() {
  const app = useAppState();
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const fieldPending = useObservable(editor.fieldPending);
  const display = useSelectedDisplay();
  const layout = draft.current.layout;
  const readOnly = editor.isReadOnly;
  const savable = display !== undefined && canSave(draft, fieldPending, app.settings, app.platform);

  return (
    <div className="title-bar">
      {readOnly ? (
        <h1 className="layout-title">
          {layout.name}
          <span className="lock" title="Preset shapes cannot be edited. Duplicate to customize.">
            <LockIcon />
          </span>
        </h1>
      ) : (
        <input
          className="layout-title layout-title-input"
          aria-label="Layout name"
          title="Click the title to rename this layout."
          value={layout.name}
          onFocus={() => editor.beginStep()}
          onChange={(event) => editor.previewName(event.target.value)}
          onBlur={() => editor.endStep()}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.currentTarget.blur();
            }
          }}
        />
      )}
      <div className="title-actions">
        <Button
          title="Start with an empty canvas. Saved layouts are kept."
          onClick={() => editor.requestNew()}
        >
          New
        </Button>
        {readOnly && (
          <Button onClick={() => editor.duplicate(layout, optionsFor(app.settings, layout))}>
            Duplicate
          </Button>
        )}
        <Button
          variant="primary"
          disabled={!savable}
          title="Save changes and use this layout on the selected display."
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => display && void editor.save(display.id)}
        >
          Save
        </Button>
      </div>
    </div>
  );
}
