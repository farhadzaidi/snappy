import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement>;

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonProps & { variant?: "plain" | "primary" | "destructive" }
>(function Button({ variant = "plain", className = "", ...props }, ref) {
  return <button ref={ref} type="button" className={`button button-${variant} ${className}`} {...props} />;
});

/** A button showing only an icon; `label` names it for assistive technology and the tooltip. */
export function IconButton({ label, className = "", ...props }: ButtonProps & { label: string }) {
  return (
    <button
      type="button"
      className={`icon-button ${className}`}
      aria-label={label}
      title={label}
      {...props}
    />
  );
}

/**
 * A toolbar button: an icon followed by a short label saying what it does. A compact one
 * shows only the icon, and the label names it for assistive technology and the tooltip.
 */
export function ToolButton({
  icon,
  label,
  compact = false,
  className = "",
  ...props
}: ButtonProps & { icon: ReactNode; label: string; compact?: boolean }) {
  return (
    <button
      type="button"
      className={`tool-button ${compact ? "tool-button-compact" : ""} ${className}`}
      aria-label={compact ? label : undefined}
      title={compact ? label : undefined}
      {...props}
    >
      {icon}
      {!compact && <span>{label}</span>}
    </button>
  );
}
