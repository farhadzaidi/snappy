// Line icons drawn on a 16-unit grid. Decorative: the control around each icon carries the label.

import type { ReactNode } from "react";

function Icon({ size = 16, children }: { size?: number; children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export const LogoIcon = () => (
  <svg width={22} height={22} viewBox="0 0 22 22" aria-hidden="true" focusable="false">
    <rect x="1" y="3" width="6" height="16" rx="1.6" fill="currentColor" />
    <rect x="8" y="3" width="6" height="16" rx="1.6" fill="currentColor" />
    <rect x="15" y="3" width="6" height="16" rx="1.6" fill="currentColor" />
  </svg>
);

export const CheckCircleIcon = ({ size }: { size?: number }) => (
  <Icon {...(size === undefined ? {} : { size })}>
    <circle cx="8" cy="8" r="6.25" fill="currentColor" stroke="none" />
    <path d="M5.2 8.2l1.9 1.9 3.7-4" stroke="var(--background)" />
  </Icon>
);

export const CheckIcon = () => (
  <Icon>
    <path d="M3.5 8.5l3 3 6-7" />
  </Icon>
);

export const PlusIcon = () => (
  <Icon>
    <path d="M8 3v10M3 8h10" />
  </Icon>
);

export const ChevronDownIcon = () => (
  <Icon size={12}>
    <path d="M4 6l4 4 4-4" />
  </Icon>
);

export const CloseIcon = () => (
  <Icon size={12}>
    <path d="M4 4l8 8M12 4l-8 8" />
  </Icon>
);

export const HelpIcon = () => (
  <Icon size={20}>
    <circle cx="8" cy="8" r="6.5" />
    <path d="M6.3 6.2a1.8 1.8 0 113 1.4c-.7.4-1.3.8-1.3 1.6M8 11.2v.1" />
  </Icon>
);

export const PlayIcon = () => (
  <svg width={9} height={9} viewBox="0 0 10 10" aria-hidden="true" focusable="false">
    <path d="M2 1l7 4-7 4z" fill="currentColor" />
  </svg>
);

export const PauseIcon = () => (
  <svg width={9} height={9} viewBox="0 0 10 10" aria-hidden="true" focusable="false">
    <rect x="1.5" y="1" width="2.5" height="8" rx="0.6" fill="currentColor" />
    <rect x="6" y="1" width="2.5" height="8" rx="0.6" fill="currentColor" />
  </svg>
);

export const RefreshIcon = () => (
  <Icon size={10}>
    <path d="M13 8a5 5 0 11-1.5-3.6M13 2.5v3h-3" />
  </Icon>
);

export const EyeIcon = () => (
  <Icon>
    <path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8s-2.4 4.5-6.5 4.5S1.5 8 1.5 8z" />
    <circle cx="8" cy="8" r="2" />
  </Icon>
);

export const EyeOffIcon = () => (
  <Icon>
    <path d="M2.5 2.5l11 11M6.6 3.7c.4-.1.9-.2 1.4-.2 4.1 0 6.5 4.5 6.5 4.5a12 12 0 01-1.7 2.3M4.3 4.9C2.5 6.1 1.5 8 1.5 8s2.4 4.5 6.5 4.5c1.3 0 2.4-.4 3.3-1" />
    <path d="M6.6 6.7a2 2 0 002.7 2.7" />
  </Icon>
);

export const PencilIcon = () => (
  <Icon>
    <path d="M10.5 2.5l3 3-8 8H2.5v-3z" />
  </Icon>
);

export const KeyboardIcon = () => (
  <Icon>
    <rect x="1.5" y="4" width="13" height="8" rx="1.5" />
    <path d="M4 6.5h.01M6.5 6.5h.01M9 6.5h.01M11.5 6.5h.01M5 9.5h6" />
  </Icon>
);

export const DuplicateIcon = () => (
  <Icon>
    <rect x="5" y="5" width="8.5" height="8.5" rx="1.5" />
    <path d="M10.5 5V3.5A1.5 1.5 0 009 2H3.5A1.5 1.5 0 002 3.5V9a1.5 1.5 0 001.5 1.5H5" />
  </Icon>
);

export const PowerIcon = () => (
  <Icon>
    <path d="M8 1.8v5.7M4.5 3.9a5.5 5.5 0 107 0" />
  </Icon>
);

export const TrashIcon = () => (
  <Icon>
    <path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4M6.7 6.5v4.5M9.3 6.5v4.5" />
  </Icon>
);

export const LockIcon = () => (
  <Icon size={12}>
    <rect x="3" y="7" width="10" height="7" rx="1.5" fill="currentColor" stroke="none" />
    <path d="M5 7V5a3 3 0 016 0v2" />
  </Icon>
);

export const CursorIcon = () => (
  <Icon size={14}>
    <path d="M3 2l9.5 5.5-4.2 1-2 4.3z" />
  </Icon>
);

export const SplitVerticalIcon = () => (
  <Icon size={14}>
    <rect x="1.5" y="2.5" width="13" height="11" rx="1.5" />
    <path d="M8 2.5v11" />
  </Icon>
);

export const SplitHorizontalIcon = () => (
  <Icon size={14}>
    <rect x="1.5" y="2.5" width="13" height="11" rx="1.5" />
    <path d="M1.5 8h13" />
  </Icon>
);

export const UndoIcon = () => (
  <Icon>
    <path d="M5.5 3L2.5 6l3 3M2.5 6h7a4 4 0 010 8H7" />
  </Icon>
);

export const RedoIcon = () => (
  <Icon>
    <path d="M10.5 3l3 3-3 3M13.5 6h-7a4 4 0 000 8H9" />
  </Icon>
);

export const AlertIcon = () => (
  <Icon>
    <circle cx="8" cy="8" r="6.5" />
    <path d="M8 4.8v3.8M8 11.1v.1" />
  </Icon>
);

export const EmptyLayoutsIcon = () => (
  <svg
    width={34}
    height={34}
    viewBox="0 0 34 34"
    fill="none"
    stroke="currentColor"
    strokeWidth={1.6}
    aria-hidden="true"
  >
    <rect x="3" y="8" width="22" height="18" rx="3" />
    <rect x="9" y="3" width="22" height="18" rx="3" />
  </svg>
);

export const ArrowIcon = ({ direction }: { direction: "left" | "right" | "up" | "down" }) => {
  const rotation = { right: 0, down: 90, left: 180, up: 270 }[direction];
  return (
    <Icon size={14}>
      <g transform={`rotate(${rotation} 8 8)`}>
        <path d="M2.5 8h11M9.5 4l4 4-4 4" />
      </g>
    </Icon>
  );
};
