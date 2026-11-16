import { useId, type ReactNode } from "react";

import { InfoHint } from "./InfoHint";

/**
 * A titled panel with a one-line explanation under its title, the longer one behind an ⓘ
 * beside the title, and an `action` for the whole panel, such as resetting it, at the
 * title's end.
 */
export function Group({
  title,
  caption,
  info,
  action,
  children,
}: {
  title: string;
  caption?: ReactNode;
  info?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}) {
  const titleId = useId();
  return (
    <section className="group" aria-labelledby={titleId}>
      <header className="group-header">
        <div className="group-heading">
          <h2 id={titleId} className="group-title">
            {title}
          </h2>
          {info && <InfoHint topic={title}>{info}</InfoHint>}
          {action && <div className="group-action">{action}</div>}
        </div>
        {caption && <p className="group-caption">{caption}</p>}
      </header>
      <div className="group-body">{children}</div>
    </section>
  );
}

/**
 * A label on the left, with an optional hint under it, and its control on the right. A
 * control that the hint describes points to it with `hintId`.
 */
export function Row({
  label,
  hint,
  hintId,
  children,
}: {
  label: ReactNode;
  hint?: ReactNode;
  hintId?: string;
  children: ReactNode;
}) {
  return (
    <div className="row">
      <span className="row-label">
        {label}
        {hint && (
          <span id={hintId} className="row-hint">
            {hint}
          </span>
        )}
      </span>
      <div className="row-control">{children}</div>
    </div>
  );
}
