import { forwardRef, type ButtonHTMLAttributes } from "react";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";

export type ButtonVariant = "secondary" | "primary" | "plain" | "link" | "destructive" | "stop";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** primary = ink (black in light, white in dark). The accent colour is never used for buttons. */
  variant?: ButtonVariant;
  size?: "md" | "sm";
  icon?: IconName;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", icon, className, children, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={cn("k-btn", variant !== "secondary" && `k-btn--${variant}`, size === "sm" && "k-btn--sm", className)}
      {...rest}
    >
      {icon && <Icon name={icon} />}
      {children}
    </button>
  );
});

export interface IconButtonProps extends Omit<ButtonProps, "icon" | "children"> {
  icon: IconName;
  /** Required: icon-only buttons need an accessible name. */
  label: string;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { icon, label, variant = "plain", size = "md", className, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={label}
      className={cn(
        "k-btn k-btn--icon",
        variant !== "secondary" && `k-btn--${variant}`,
        size === "sm" && "k-btn--sm",
        className,
      )}
      {...rest}
    >
      <Icon name={icon} />
    </button>
  );
});
