import { Alert, Button, CloseButton, Label, ListBox, Select } from "@heroui/react";
import { LuArrowLeft, LuX } from "react-icons/lu";
import type { ReactNode } from "react";
import { useLogicalNavigation } from "../logical-navigation";
export function ModuleHeader({
  title,
  description,
  backLabel = "返回工作台",
  actions,
}: {
  title: string;
  description?: string;
  backLabel?: string;
  actions?: ReactNode;
}) {
  const nav = useLogicalNavigation();
  return (
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 items-center gap-3">
        <CloseButton data-webview-navigation aria-label={backLabel} onPress={() => void nav.back()}>
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <div>
          <h1 data-webview-navigation className="text-2xl font-semibold">
            {title}
          </h1>
          {description && <p className="text-sm text-muted">{description}</p>}
        </div>
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}
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
