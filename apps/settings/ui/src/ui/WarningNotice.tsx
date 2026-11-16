import { AlertIcon } from "./icons";
import { MessageText } from "./MessageText";

/**
 * A problem shown where it applies, such as why a change was refused, pointing to the setting
 * titled `setting` where the message names it.
 */
export function WarningNotice({ message, setting = null }: { message: string; setting?: string | null }) {
  return (
    <p className="warning-notice" role="alert">
      <span className="warning-notice-icon">
        <AlertIcon />
      </span>
      <MessageText text={message} setting={setting} />
    </p>
  );
}
