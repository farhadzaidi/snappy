import type { Remedy } from "../core/model";
import { useAppState, useSend } from "../state/services";
import { Button } from "../ui/Button";
import { AlertIcon, InfoIcon } from "../ui/icons";

const REMEDY_LABELS: Record<Remedy, string> = {
  logOut: "Log out now",
};

/**
 * Why Snappy can't control windows here, with a button when Snappy can fix it, and notes
 * about the desktop, such as another tool that also snaps windows.
 */
export function ShellNotices() {
  const { snapping, platformNotes } = useAppState();
  const send = useSend();
  const remedy = snapping.unavailableRemedy;
  return (
    <>
      {snapping.unavailableReason !== null && (
        <div className="shell-notice" role="status">
          <span className="shell-notice-icon">
            <AlertIcon />
          </span>
          <span className="shell-notice-text">{snapping.unavailableReason}</span>
          {remedy !== null && (
            <Button variant="primary" onClick={() => void send({ kind: "applyRemedy", remedy })}>
              {REMEDY_LABELS[remedy]}
            </Button>
          )}
        </div>
      )}
      {platformNotes.map((note) => (
        <div key={note} className="shell-notice platform-note" role="note">
          <span className="platform-note-icon">
            <InfoIcon />
          </span>
          <span className="shell-notice-text">{note}</span>
        </div>
      ))}
    </>
  );
}
