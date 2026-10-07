import { Alert, Button, CloseButton, Label, ListBox, Select } from "@heroui/react";
import { LuX } from "react-icons/lu";
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
  if (!message) return null;
  return (
    <Alert status="warning">
      <Alert.Content>
        <Alert.Title>{message}</Alert.Title>
        {onRetry && (
          <Button size="sm" variant="secondary" onPress={onRetry}>
            重试
          </Button>
        )}
      </Alert.Content>
      {onDismiss && (
        <CloseButton aria-label="关闭提示" onPress={onDismiss}>
          <LuX />
        </CloseButton>
      )}
    </Alert>
  );
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
