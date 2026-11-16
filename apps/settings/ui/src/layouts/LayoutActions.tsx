import { useRef, useState, type MouseEvent, type RefObject } from "react";

import { isEnabled, isPreset, optionsFor } from "../core/layouts";
import type { Display, Layout } from "../core/model";
import { useAppState, useSelectedDisplay, useSend, useServices } from "../state/services";
import { ActionBar, type BarAction } from "../ui/ActionBar";
import { ToolButton } from "../ui/Button";
import { ConfirmPopover } from "../ui/ConfirmPopover";
import {
  DuplicateIcon,
  EyeIcon,
  EyeOffIcon,
  KeyboardIcon,
  PencilIcon,
  PowerIcon,
  TrashIcon,
} from "../ui/icons";

/** Groups of layout actions: viewing and editing, then managing. */
const VIEW = 0;
const MANAGE = 1;

/**
 * One action bar holding every layout action, always in the same order: enabling or
 * disabling the layout first, then viewing and editing, then deleting.
 */
export function LayoutActions({ layout }: { layout: Layout }) {
  const { editor } = useServices();
  const display = useSelectedDisplay();
  const preview = usePreviewToggle(layout, display);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const deleteButton = useRef<HTMLButtonElement | null>(null);
  const app = useAppState();
  const preset = isPreset(layout.id);
  const options = optionsFor(app.settings, layout);

  const actions: BarAction[] = [
    {
      id: "preview",
      group: VIEW,
      label: preview.visible ? "Hide zones" : "Show on screen",
      icon: preview.visible ? <EyeOffIcon /> : <EyeIcon />,
      pressed: preview.visible,
      disabled: !display,
      onPress: preview.press,
      onSelect: preview.toggle,
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
  ];
  if (!preset) {
    actions.push({
      id: "delete",
      group: MANAGE,
      label: "Delete",
      icon: <TrashIcon />,
      destructive: true,
      onSelect: (event) => {
        deleteButton.current = event.currentTarget;
        setConfirmingDelete(!confirmingDelete);
      },
    });
  }

  return (
    <ActionBar label="Layout actions" lead={<EnableButton layout={layout} />} actions={actions}>
      {(bar) =>
        confirmingDelete && (
          <DeleteConfirmation
            layout={layout}
            anchor={bar}
            trigger={deleteButton}
            onClose={() => setConfirmingDelete(false)}
          />
        )
      }
    </ActionBar>
  );
}

/** Enables a disabled layout, standing out until it is on, or quietly disables an enabled one. */
function EnableButton({ layout }: { layout: Layout }) {
  const app = useAppState();
  const send = useSend();
  const enabled = isEnabled(app.settings, layout);
  return (
    <ToolButton
      icon={<PowerIcon />}
      label={enabled ? "Disable" : "Enable"}
      className={enabled ? "" : "tool-button-primary"}
      onClick={() => void send({ kind: "setLayoutEnabled", layout: layout.id, enabled: !enabled })}
    />
  );
}

/**
 * Shows the layout's zones on the display, or hides them. The daemon hides the preview at
 * every press, this button's included, so by the time a click arrives the state may already
 * say hidden; a click acts on what the button showed when it was pressed.
 */
function usePreviewToggle(layout: Layout, display: Display | undefined) {
  const send = useSend();
  const visible = useAppState().snapping.previewVisible;
  const visibleAtPress = useRef(visible);

  return {
    visible,
    press: () => {
      visibleAtPress.current = visible;
    },
    toggle: (event: MouseEvent<HTMLButtonElement>) => {
      // A click from the keyboard has no press before it.
      const shown = event.detail > 0 ? visibleAtPress.current : visible;
      if (shown) {
        void send({ kind: "hidePreview" });
      } else if (display) {
        void send({ kind: "showPreview", display: display.id, layout: layout.id });
      }
    },
  };
}

/** Asks before deleting a custom layout, just above the bar's end. */
function DeleteConfirmation({
  layout,
  anchor,
  trigger,
  onClose,
}: {
  layout: Layout;
  anchor: RefObject<HTMLDivElement | null>;
  trigger: RefObject<HTMLButtonElement | null>;
  onClose: () => void;
}) {
  const send = useSend();

  return (
    <ConfirmPopover
      label="Delete layout"
      question={`Delete ${layout.name}?`}
      confirmTitle="Delete"
      anchor={anchor}
      trigger={trigger}
      placement="above"
      onConfirm={() => void send({ kind: "deleteLayout", layout: layout.id })}
      onClose={onClose}
    />
  );
}
