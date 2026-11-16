// Putting every setting, or everything, back to its defaults from the end of the Settings
// page. Each reset is one command the daemon validates like any change.

import { useState } from "react";

import type { Settings } from "../core/model";
import { core } from "../core/snappyCore";
import { useAppState, useSend } from "../state/services";
import { Button } from "../ui/Button";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { Group, Row } from "../ui/Group";
import { RefreshIcon, TrashIcon } from "../ui/icons";

type Confirming = "settings" | "everything" | null;

/** Resetting every setting and shortcut, and, separately, erasing everything. */
export function ResetGroup() {
  const { settings, platform, workspaces } = useAppState();
  const send = useSend();
  const [confirming, setConfirming] = useState<Confirming>(null);
  const changes = (scope: "settings" | "everything") =>
    core.resetSettings(settings, { scope }, platform).changed;
  const reset = (scope: "settings" | "everything") => {
    setConfirming(null);
    void send({ kind: "reset", reset: { scope } });
  };

  return (
    <Group
      title="Reset"
      caption="Go back to Snappy's defaults."
      info="Resetting can't be undone. Export settings first to keep a copy."
    >
      <Row label="Settings and shortcuts" hint="Your layouts and workspaces stay.">
        <Button disabled={!changes("settings")} onClick={() => setConfirming("settings")}>
          Reset
        </Button>
      </Row>
      <Row label="Everything" hint="Also removes your layouts and workspaces.">
        <Button
          variant="destructive"
          disabled={!changes("everything")}
          onClick={() => setConfirming("everything")}
        >
          Reset everything
        </Button>
      </Row>
      {confirming === "settings" && (
        <ConfirmDialog
          title="Reset settings and shortcuts?"
          message="Every setting and shortcut goes back to its default, layout and zone shortcuts too. Your layouts and workspaces stay."
          confirmTitle="Reset"
          icon={<RefreshIcon size={22} />}
          onCancel={() => setConfirming(null)}
          onConfirm={() => reset("settings")}
        />
      )}
      {confirming === "everything" && (
        <ConfirmDialog
          title="Reset everything?"
          message={erasing(settings, workspaces.running)}
          confirmTitle="Reset everything"
          icon={<TrashIcon />}
          onCancel={() => setConfirming(null)}
          onConfirm={() => reset("everything")}
        />
      )}
    </Group>
  );
}

function counted(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

/** What erasing everything loses, by name and number. */
function erasing(settings: Settings, workspacesRunning: boolean): string {
  const custom = settings.layouts.custom.length;
  const lost = [
    ...(custom > 0 ? [counted(custom, "custom layout")] : []),
    counted(settings.workspaces.items.length, "workspace"),
  ].join(" and ");
  const windows = workspacesRunning ? " Windows in other workspaces come back to the screen." : "";
  return `This removes your ${lost}, and every setting and shortcut goes back to its default.${windows}`;
}
