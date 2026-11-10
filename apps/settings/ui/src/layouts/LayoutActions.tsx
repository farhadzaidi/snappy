import { useState, type RefObject } from "react";

import { isEnabled, isPreset, optionsFor } from "../core/layouts";
import type { Layout } from "../core/model";
import { useAppState, useSelectedDisplay, useSend, useServices } from "../state/services";
import { ActionBar, type BarAction } from "../ui/ActionBar";
import { Button } from "../ui/Button";
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

/** Groups of layout actions: viewing and editing, then managing. */
const VIEW = 0;
const MANAGE = 1;

/** One action bar holding every layout action: the primary "Use layout", then viewing and
 * editing, then managing. What doesn't fit moves into a "More actions" menu. */
export function LayoutActions({ layout, inUse }: { layout: Layout; inUse: boolean }) {
  const app = useAppState();
  const { editor } = useServices();
  const send = useSend();
  const display = useSelectedDisplay();
  const [confirmingDelete, setConfirmingDelete] = useState(false);
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

  const actions: BarAction[] = [
    {
      id: "preview",
      group: VIEW,
      label: previewVisible ? "Hide zones" : "Show on screen",
      icon: previewVisible ? <EyeOffIcon /> : <EyeIcon />,
      pressed: previewVisible,
      disabled: !display,
      onSelect: togglePreview,
    },
    {
      id: "edit",
      group: VIEW,
      label: preset ? "Shortcuts" : "Edit",
      icon: preset ? <KeyboardIcon /> : <PencilIcon />,
      onSelect: () => editor.open(layout, options),
    },
    {
      id: "duplicate",
      group: VIEW,
      label: "Duplicate",
      icon: <DuplicateIcon />,
      onSelect: () => editor.duplicate(layout, options),
    },
    {
      id: "enable",
      group: MANAGE,
      label: enabled ? "Disable" : "Enable",
      icon: <PowerIcon />,
      onSelect: () => setEnabled(!enabled),
    },
  ];
  if (!preset) {
    actions.push({
      id: "delete",
      group: MANAGE,
      label: "Delete",
      icon: <TrashIcon />,
      destructive: true,
      onSelect: () => setConfirmingDelete(!confirmingDelete),
    });
  }

  return (
    <ActionBar
      label="Layout actions"
      primary={
        !inUse && (
          <Button variant="primary" className="action-primary" onClick={primaryAction}>
            {enabled ? "Use layout" : "Enable layout"}
          </Button>
        )
      }
      actions={actions}
    >
      {(bar) =>
        confirmingDelete && (
          <DeleteConfirmation layout={layout} anchor={bar} onClose={() => setConfirmingDelete(false)} />
        )
      }
    </ActionBar>
  );
}

/** Asks before deleting a custom layout, just above the bar's end. */
function DeleteConfirmation({
  layout,
  anchor,
  onClose,
}: {
  layout: Layout;
  anchor: RefObject<HTMLDivElement | null>;
  onClose: () => void;
}) {
  const send = useSend();

  return (
    <Popover
      label="Delete layout"
      anchor={anchor}
      align="end"
      placement="above"
      onClose={onClose}
      className="confirm-popover"
    >
      <p className="confirm-title">Delete {layout.name}?</p>
      <div className="confirm-actions">
        <Button autoFocus onClick={onClose}>
          Cancel
        </Button>
        <Button
          variant="destructive"
          onClick={() => {
            onClose();
            void send({ kind: "deleteLayout", layout: layout.id });
          }}
        >
          Delete
        </Button>
      </div>
    </Popover>
  );
}
