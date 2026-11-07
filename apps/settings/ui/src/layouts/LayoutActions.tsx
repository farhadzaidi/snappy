import { useRef, useState } from "react";

import { isEnabled, isPreset, optionsFor } from "../core/layouts";
import type { Layout } from "../core/model";
import { useAppState, useSelectedDisplay, useSend, useServices } from "../state/services";
import { Button, ToolButton } from "../ui/Button";
import {
  DuplicateIcon,
  EyeIcon,
  EyeOffIcon,
  KeyboardIcon,
  PencilIcon,
  PowerIcon,
  TrashIcon,
} from "../ui/icons";
import { Popover } from "../ui/Popover";

/** One action bar holding every layout action, grouped as using, viewing and
 * editing, and managing. */
export function LayoutActions({ layout, inUse }: { layout: Layout; inUse: boolean }) {
  const app = useAppState();
  const { editor } = useServices();
  const send = useSend();
  const display = useSelectedDisplay();
  const enabled = isEnabled(app.settings, layout);
  const preset = isPreset(layout.id);
  const options = optionsFor(app.settings, layout);
  const previewVisible = app.snapping.previewVisible;

  const setEnabled = (value: boolean) =>
    void send({ kind: "setLayoutEnabled", layout: layout.id, enabled: value });
  /** "Use layout" applies it; a disabled layout reads "Enable layout" and enables it. */
  const primaryAction = () => {
    if (!enabled) {
      setEnabled(true);
    } else if (display) {
      void send({ kind: "applyLayout", display: display.id, layout: layout.id });
    }
  };
  const togglePreview = () => {
    if (previewVisible) {
      void send({ kind: "hidePreview" });
    } else if (display) {
      void send({ kind: "showPreview", display: display.id, layout: layout.id });
    }
  };

  return (
    <div className="action-bar" role="toolbar" aria-label="Layout actions">
      {!inUse && (
        <Button variant="primary" className="action-primary" onClick={primaryAction}>
          {enabled ? "Use layout" : "Enable layout"}
        </Button>
      )}
      <div className="action-group">
        <ToolButton
          icon={previewVisible ? <EyeOffIcon /> : <EyeIcon />}
          aria-pressed={previewVisible}
          disabled={!display}
          onClick={togglePreview}
        >
          {previewVisible ? "Hide zones" : "Show on screen"}
        </ToolButton>
        <ToolButton
          icon={preset ? <KeyboardIcon /> : <PencilIcon />}
          onClick={() => editor.open(layout, options)}
        >
          {preset ? "Shortcuts" : "Edit"}
        </ToolButton>
        <ToolButton icon={<DuplicateIcon />} onClick={() => editor.duplicate(layout, options)}>
          Duplicate
        </ToolButton>
      </div>
      <div className="action-divider" aria-hidden="true" />
      <div className="action-group">
        <ToolButton icon={<PowerIcon />} onClick={() => setEnabled(!enabled)}>
          {enabled ? "Disable" : "Enable"}
        </ToolButton>
        {!preset && <DeleteLayoutButton layout={layout} />}
      </div>
    </div>
  );
}

/** "Delete layout" asks for confirmation in a popover. */
function DeleteLayoutButton({ layout }: { layout: Layout }) {
  const [confirming, setConfirming] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const send = useSend();

  return (
    <div className="popover-anchor" ref={anchor}>
      <ToolButton
        icon={<TrashIcon />}
        className="tool-button-destructive"
        aria-expanded={confirming}
        onClick={() => setConfirming(!confirming)}
      >
        Delete
      </ToolButton>
      {confirming && (
        <Popover
          label="Delete layout"
          anchor={anchor}
          align="start"
          placement="above"
          onClose={() => setConfirming(false)}
          className="confirm-popover"
        >
          <p className="confirm-title">Delete {layout.name}?</p>
          <div className="confirm-actions">
            <Button autoFocus onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setConfirming(false);
                void send({ kind: "deleteLayout", layout: layout.id });
              }}
            >
              Delete
            </Button>
          </div>
        </Popover>
      )}
    </div>
  );
}
