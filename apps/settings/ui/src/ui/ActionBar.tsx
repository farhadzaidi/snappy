import {
  Fragment,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
  type RefObject,
} from "react";

import { ToolButton } from "./Button";
import { useElementSize } from "./useElementSize";

/** One action in an action bar. Actions sharing a `group` sit together, with a divider
 * between groups. */
export interface BarAction {
  id: string;
  label: string;
  icon: ReactNode;
  group: number;
  onSelect: (event: MouseEvent<HTMLButtonElement>) => void;
  /** Called when the pointer goes down on the action, before it is selected. */
  onPress?: () => void;
  pressed?: boolean;
  disabled?: boolean;
  destructive?: boolean;
}

interface ActionBarProps {
  label: string;
  /** A control before the actions that always keeps its label. */
  lead?: ReactNode;
  actions: BarAction[];
  /** Panels that belong to the bar, such as a confirmation, given the bar to anchor to. */
  children?: (bar: RefObject<HTMLDivElement | null>) => ReactNode;
}

/**
 * A toolbar as wide as its column that keeps every action on one row, in the same order:
 * with their labels when they all fit, otherwise as icons alone. The labelled row is laid out in an invisible copy,
 * so the bar decides from its measured width rather than a guess.
 */
export function ActionBar({ label, lead, actions, children }: ActionBarProps) {
  const frame = useRef<HTMLDivElement>(null);
  const bar = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const available = useElementSize(frame).width;
  // Fonts loading change the measured width, and so this size.
  const labelledWidth = useElementSize(measure).width;
  const [compact, setCompact] = useState(false);

  useLayoutEffect(() => {
    setCompact(labelledWidth > available);
  }, [available, labelledWidth]);

  return (
    <div className="action-bar-frame" ref={frame}>
      <div className="action-bar-anchor" ref={bar}>
        <BarRow label={label} lead={lead} actions={actions} compact={compact} />
        {children?.(bar)}
      </div>
      <div className="action-bar-measure" ref={measure} aria-hidden="true">
        <BarRow lead={lead} actions={actions} compact={false} />
      </div>
    </div>
  );
}

/**
 * The bar's row, its buttons sharing its width evenly: a faint line between neighbors, and
 * a plain one between groups. Without a label it is the copy for measuring, which nobody
 * sees or reaches.
 */
function BarRow({
  label,
  lead,
  actions,
  compact,
}: {
  label?: string;
  lead?: ReactNode;
  actions: BarAction[];
  compact: boolean;
}) {
  return (
    <div className="action-bar" role={label ? "toolbar" : undefined} aria-label={label}>
      {lead}
      {actions.map((action, index) => {
        const previous = actions[index - 1];
        const newGroup = previous !== undefined && previous.group !== action.group;
        return (
          <Fragment key={action.id}>
            {(lead !== undefined || previous !== undefined) && (
              <span className={newGroup ? "action-divider" : "action-separator"} aria-hidden="true" />
            )}
            <ToolButton
              icon={action.icon}
              label={action.label}
              compact={compact}
              aria-pressed={action.pressed}
              disabled={action.disabled}
              className={action.destructive ? "tool-button-destructive" : ""}
              onPointerDown={action.onPress}
              onClick={action.onSelect}
            />
          </Fragment>
        );
      })}
    </div>
  );
}
