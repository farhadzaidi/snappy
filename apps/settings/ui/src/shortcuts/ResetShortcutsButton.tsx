import type { Reset } from "../core/model";
import { core } from "../core/snappyCore";
import { useAppState, useSend } from "../state/services";
import { Reference } from "../ui/Reference";
import { ResetButton } from "../ui/ResetButton";

/**
 * Resets the shortcuts of one panel, named `title`, after asking: a section of the Settings
 * page or a workspace's own. Off while they are the defaults, as `snappy-core` says.
 */
export function ResetShortcutsButton({ reset, title }: { reset: Reset; title: string }) {
  const { settings, platform } = useAppState();
  const send = useSend();
  const { changed } = core.resetSettings(settings, reset, platform);
  return (
    <ResetButton
      label={`Reset ${title} shortcuts`}
      question={
        <>
          Reset <Reference>{title}</Reference> shortcuts to their defaults?
        </>
      }
      disabled={!changed}
      onConfirm={() => void send({ kind: "reset", reset })}
    />
  );
}
