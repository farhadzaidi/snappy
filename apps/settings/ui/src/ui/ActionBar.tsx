import { Fragment, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";

import { IconButton, ToolButton } from "./Button";
import { MoreIcon } from "./icons";
import { Popover } from "./Popover";
import { useElementSize } from "./useElementSize";

/** One action in an action bar. Actions sharing a `group` sit together, with a divider
 * between groups. */
export interface BarAction {
  id: string;
  label: string;
  icon: ReactNode;
  group: number;
  onSelect: () => void;
  pressed?: boolean;
  disabled?: boolean;
  destructive?: boolean;
}

interface ActionBarProps {
  label: string;
  /** A button before the actions that always stays in the bar. */
  primary?: ReactNode;
  actions: BarAction[];
  /** Panels that belong to the bar, such as a confirmation, given the bar to anchor to. */
  children?: (bar: RefObject<HTMLDivElement | null>) => ReactNode;
}

/**
 * A toolbar that stays on one row: actions that don't fit move, the last first, into a
 * "More actions" menu. Every way of fitting is laid out in an invisible copy, so the bar
 * picks from measured widths rather than guesses.
 */
export function ActionBar({ label, primary, actions, children }: ActionBarProps) {
  const frame = useRef<HTMLDivElement>(null);
  const bar = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const available = useElementSize(frame).width;
  // Fonts loading change the measured widths, and so this size.
  const measured = useElementSize(measure).width;
  const [shown, setShown] = useState(actions.length);
  const labels = actions.map((action) => action.label).join("\n");

  useLayoutEffect(() => {
    const rows = Array.from(measure.current?.children ?? []);
    // Row i lays out all but the last i actions.
    const fits = rows.findIndex((row) => (row as HTMLElement).offsetWidth <= available);
    setShown(fits === -1 ? 0 : actions.length - fits);
  }, [available, measured, labels, actions.length]);

  const counts = Array.from({ length: actions.length + 1 }, (_, index) => actions.length - index);

  return (
    <div className="action-bar-frame" ref={frame}>
      <div className="action-bar-anchor" ref={bar}>
        <BarRow label={label} primary={primary} actions={actions} shown={shown} />
        {children?.(bar)}
      </div>
      <div className="action-bar-measure" ref={measure} aria-hidden="true">
        {counts.map((count) => (
          <BarRow key={count} primary={primary} actions={actions} shown={count} />
        ))}
      </div>
    </div>
  );
}

/** The bar with its first `shown` actions in line and the rest in the menu. Without a
 * label it is a copy for measuring, which nobody sees or reaches. */
function BarRow({
  label,
  primary,
  actions,
  shown,
}: {
  label?: string;
  primary?: ReactNode;
  actions: BarAction[];
  shown: number;
}) {
  const inline = actions.slice(0, shown);
  const overflow = actions.slice(shown);
  const groups = [...new Set(inline.map((action) => action.group))];

  return (
    <div className="action-bar" role={label ? "toolbar" : undefined} aria-label={label}>
      {primary}
      {groups.map((group, index) => (
        <Fragment key={group}>
          {index > 0 && <div className="action-divider" aria-hidden="true" />}
          <div className="action-group">
            {inline
              .filter((action) => action.group === group)
              .map((action) => (
                <ToolButton
                  key={action.id}
                  icon={action.icon}
                  aria-pressed={action.pressed}
                  disabled={action.disabled}
                  className={action.destructive ? "tool-button-destructive" : ""}
                  onClick={action.onSelect}
                >
                  {action.label}
                </ToolButton>
              ))}
          </div>
        </Fragment>
      ))}
      {overflow.length > 0 && <MoreMenu actions={overflow} />}
    </div>
  );
}

/** The "More actions" button and the menu of actions that didn't fit. */
function MoreMenu({ actions }: { actions: BarAction[] }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);

  return (
    <div className="popover-anchor" ref={anchor}>
      <IconButton
        label="More actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <MoreIcon />
      </IconButton>
      {open && (
        <Popover
          label="More actions"
          role="menu"
          anchor={anchor}
          align="end"
          onClose={() => setOpen(false)}
          className="action-menu"
        >
          {actions.map((action) => (
            <button
              type="button"
              role="menuitem"
              key={action.id}
              className={`action-menu-item ${action.destructive ? "action-menu-item-destructive" : ""}`}
              disabled={action.disabled}
              onClick={() => {
                setOpen(false);
                action.onSelect();
              }}
            >
              {action.icon}
              <span>{action.label}</span>
            </button>
          ))}
        </Popover>
      )}
    </div>
  );
}
