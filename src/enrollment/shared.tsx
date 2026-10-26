import { useEffect, useRef } from "react";
import { Select, SelectOption } from "material/react";
import { toast } from "../ui";
export { ModuleHeader } from "../components";
export function OperationError({
  message,
  onRetry,
  onDismiss,
}: {
  message?: string;
  onRetry?: () => void;
  onDismiss?: () => void;
}) {
  const lastToastRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (message && message !== lastToastRef.current) {
      lastToastRef.current = message;
      toast.danger(message, {
        actionLabel: onRetry ? "重试" : undefined,
        onAction: onRetry,
        onClose: onDismiss,
      });
    }
  }, [message, onRetry, onDismiss]);

  return null;
}
export function Choice({
  label,
  value,
  onChange,
  items,
  disabled = false,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  items: { id: string; label: string }[];
  disabled?: boolean;
}) {
  return (
    <Select
      label={label}
      aria-label={label}
      value={value || ""}
      disabled={disabled}
      onChange={(e: any) => {
        const val = e.detail?.value ?? e.target?.value;
        onChange(String(val ?? ""));
      }}
    >
      {items.map((i) => (
        <SelectOption key={i.id} value={i.id}>
          {i.label}
        </SelectOption>
      ))}
    </Select>
  );
}
export const categories: Record<string, string> = {
  TJKC: "本班推荐",
  FANKC: "方案内课程",
  FAWKC: "方案外课程",
  XGXK: "通识选修",
  TYKC: "体育课程",
  FXKC: "辅修课程",
  MOOC: "在线课程",
};
export const phases: Record<string, string> = {
  pending: "等待开始",
  monitoring: "监控余量",
  dropping: "确认退课",
  selecting: "选课中",
  restoring: "回抢旧课",
  uncertain: "待核验",
  completed: "已完成",
  stopped: "已停止",
};
