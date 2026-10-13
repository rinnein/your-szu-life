import { useEffect, useRef } from "react";
import { Label, ListBox, Select, toast } from "@heroui/react";
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
        actionProps: onRetry
          ? {
              children: "重试",
              onPress: onRetry,
            }
          : undefined,
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
      aria-label={label}
      value={value || null}
      onChange={(key) => onChange(String(key ?? ""))}
      isDisabled={disabled}
    >
      <Label>{label}</Label>
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {items.map((i) => (
            <ListBox.Item key={i.id} id={i.id} textValue={i.label}>
              {i.label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
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
