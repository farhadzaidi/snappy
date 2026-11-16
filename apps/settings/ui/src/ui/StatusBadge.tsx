import type { ReactNode } from "react";

import { AlertIcon, CheckCircleIcon } from "./icons";

/** A short status, such as "Allowed", marked good or bad by its color and its icon. */
export function StatusBadge({ good, children }: { good: boolean; children: ReactNode }) {
  return (
    <span className="status-badge" data-good={good}>
      {good ? <CheckCircleIcon size={14} /> : <AlertIcon size={14} />}
      {children}
    </span>
  );
}
