import type { NamedShortcut } from "../core/model";
import { AlertIcon } from "./icons";
import { MessageText } from "./MessageText";

/**
 * A problem shown where it applies, such as why a change was refused, pointing to `shortcut`
 * where the message names it.
 */
export function WarningNotice({
  message,
  shortcut = null,
}: {
  message: string;
  shortcut?: NamedShortcut | null;
}) {
  return (
    <p className="warning-notice" role="alert">
      <span className="warning-notice-icon">
        <AlertIcon />
      </span>
      <MessageText text={message} shortcut={shortcut} />
    </p>
  );
}
