import { useObservable } from "../state/observable";
import { useServices } from "../state/services";
import { IconButton } from "../ui/Button";
import { AlertIcon, CheckCircleIcon, CloseIcon } from "../ui/icons";

/** Messages at the bottom of the window. */
export function Toast() {
  const { client } = useServices();
  const toast = useObservable(client.toast);
  if (!toast) {
    return null;
  }
  return (
    <div className="toast-region">
      <div
        key={toast.key}
        className="toast"
        data-error={toast.isError}
        role={toast.isError ? "alert" : "status"}
      >
        <span className="toast-icon">{toast.isError ? <AlertIcon /> : <CheckCircleIcon />}</span>
        <span className="toast-message">{toast.message}</span>
        <IconButton label="Dismiss message" className="toast-dismiss" onClick={() => client.dismissToast()}>
          <CloseIcon />
        </IconButton>
      </div>
    </div>
  );
}
