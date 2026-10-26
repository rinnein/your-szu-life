import type { ReactNode } from "react";
import { Card } from "material/react";

export function StatCard({
  label,
  value,
  unit,
  icon,
  trend,
  description,
  color = "default",
}: {
  label: string;
  value: number | string;
  unit?: string;
  icon?: ReactNode;
  trend?: string;
  description?: ReactNode;
  color?: "default" | "brand" | "success" | "warning" | "danger";
}) {
  const colors = {
    default: "text-[var(--szu-ink)]",
    brand: "text-[var(--szu-brand)]",
    success: "text-success",
    warning: "text-warning",
    danger: "text-danger",
  };

  return (
    <Card aria-label={label} variant="outlined" className="payments-card">
      <div className="py-5 px-6">
        <div className="flex items-start justify-between mb-2">
          <span className="text-sm text-[var(--szu-muted)]">{label}</span>
          {icon && <span className={colors[color]}>{icon}</span>}
        </div>
        <div className="flex items-baseline gap-1.5">
          <span className={`text-3xl font-bold tracking-tight ${colors[color]}`}>{value}</span>
          {unit && <span className="text-lg text-[var(--szu-muted)]"> {unit}</span>}
        </div>
        {trend && <div className="mt-2 text-xs text-success">{trend}</div>}
        {description && <div className="mt-2 text-xs text-[var(--szu-muted)]">{description}</div>}
      </div>
    </Card>
  );
}
