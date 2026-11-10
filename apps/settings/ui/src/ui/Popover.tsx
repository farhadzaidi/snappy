import { useEffect, useRef, type ReactNode, type RefObject } from "react";

interface PopoverProps {
  label: string;
  onClose: () => void;
  /** The element the popover belongs to; presses inside it don't close the popover. */
  anchor: RefObject<HTMLElement | null>;
  align?: "start" | "end";
  placement?: "below" | "above";
  role?: "dialog" | "listbox" | "menu";
  className?: string;
  children: ReactNode;
}

/** A panel under its anchor that closes on Escape and on presses elsewhere. */
export function Popover({
  label,
  onClose,
  anchor,
  align = "start",
  placement = "below",
  role = "dialog",
  className = "",
  children,
}: PopoverProps) {
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      if (!anchor.current?.contains(event.target as Node)) {
        onClose();
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape, true);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape, true);
    };
  }, [anchor, onClose]);

  return (
    <div
      ref={panel}
      role={role}
      aria-label={label}
      className={`popover popover-${align} popover-${placement} ${className}`}
    >
      {children}
    </div>
  );
}
