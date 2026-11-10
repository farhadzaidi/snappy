import { useId, type ReactNode } from "react";

/** A titled panel whose one-line explanation sits directly under its title. */
export function Group({
  title,
  caption,
  children,
}: {
  title: string;
  caption?: string;
  children: ReactNode;
}) {
  const titleId = useId();
  return (
    <section className="group" aria-labelledby={titleId}>
      <header className="group-header">
        <h2 id={titleId} className="group-title">
          {title}
        </h2>
        {caption && <p className="group-caption">{caption}</p>}
      </header>
      <div className="group-body">{children}</div>
    </section>
  );
}

/** A label on the left and its control on the right. */
export function Row({ label, children }: { label: ReactNode; children: ReactNode }) {
  return (
    <div className="row">
      <span className="row-label">{label}</span>
      <div className="row-control">{children}</div>
    </div>
  );
}
