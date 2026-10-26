import type { ReactNode } from "react";

export type ChipColor = "default" | "accent" | "success" | "warning" | "danger" | "info";
export type ChipVariant = "soft" | "primary" | "secondary";

export function Chip({
  children,
  color = "default",
  variant = "soft",
  size = "md",
  className,
  "aria-label": ariaLabel,
}: {
  children: ReactNode;
  color?: ChipColor | string;
  variant?: ChipVariant | string;
  size?: "sm" | "md" | "lg" | string;
  className?: string;
  "aria-label"?: string;
}) {
  return (
    <span
      className={`md-chip ${className ?? ""}`}
      data-color={color}
      data-variant={variant}
      data-size={size}
      aria-label={ariaLabel}
    >
      {children}
    </span>
  );
}
