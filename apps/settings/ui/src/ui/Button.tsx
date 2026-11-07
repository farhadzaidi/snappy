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

/** A toolbar button: an icon followed by a short label saying what it does. */
export function ToolButton({ icon, className = "", children, ...props }: ButtonProps & { icon: ReactNode }) {
  return (
    <button type="button" className={`tool-button ${className}`} {...props}>
      {icon}
      <span>{children}</span>
    </button>
  );
}
