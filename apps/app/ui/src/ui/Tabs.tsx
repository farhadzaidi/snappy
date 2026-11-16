import { useId, useRef, type KeyboardEvent } from "react";

/** The ids that tie a tab list to the one panel that shows its selected tab. */
export interface TabIds {
  tab: (value: string) => string;
  panel: string;
}

export function useTabIds(): TabIds {
  const base = useId();
  return { tab: (value) => `${base}-tab-${value}`, panel: `${base}-panel` };
}

/** What the panel showing the selected tab carries. */
export function tabPanelProps(ids: TabIds, selected: string) {
  return { role: "tabpanel", id: ids.panel, "aria-labelledby": ids.tab(selected) } as const;
}

/**
 * Segments that switch one panel between views, as a tab list: only the selected tab is in
 * the tab order, and the arrow keys, Home, and End move to and show another.
 */
export function Tabs<T extends string>({
  label,
  tabs,
  selected,
  ids,
  className = "segmented segmented-even",
  onSelect,
}: {
  label: string;
  tabs: readonly { value: T; title: string }[];
  selected: T;
  ids: TabIds;
  className?: string;
  onSelect: (value: T) => void;
}) {
  const list = useRef<HTMLDivElement>(null);

  const onKeyDown = (event: KeyboardEvent) => {
    const index = tabs.findIndex((tab) => tab.value === selected);
    const target = {
      ArrowLeft: index - 1,
      ArrowUp: index - 1,
      ArrowRight: index + 1,
      ArrowDown: index + 1,
      Home: 0,
      End: tabs.length - 1,
    }[event.key];
    if (target === undefined) {
      return;
    }
    event.preventDefault();
    const next = tabs[(target + tabs.length) % tabs.length];
    if (next) {
      onSelect(next.value);
      list.current?.querySelector<HTMLElement>(`#${CSS.escape(ids.tab(next.value))}`)?.focus();
    }
  };

  return (
    <div ref={list} className={className} role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {tabs.map(({ value, title }) => (
        <button
          type="button"
          role="tab"
          key={value}
          id={ids.tab(value)}
          className="segment"
          aria-selected={value === selected}
          aria-controls={ids.panel}
          tabIndex={value === selected ? 0 : -1}
          onClick={() => onSelect(value)}
        >
          {title}
        </button>
      ))}
    </div>
  );
}
