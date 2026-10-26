import { createContext, useId, useState, type ReactNode } from "react";

/**
 * 可展开区块：material 没有 Accordion，自建。
 *
 * material 没有 Accordion，因此自建。用 Motion 做高度动画，
 * 遵循 `prefers-reduced-motion`。
 */

interface DisclosureContextValue {
  open: boolean;
  toggle: () => void;
  panelId: string;
  triggerId: string;
}

const DisclosureContext = createContext<DisclosureContextValue | null>(null);

export function Disclosure({
  title,
  children,
  defaultOpen = false,
  open: controlledOpen,
  onOpenChange,
  ariaLabel,
}: {
  title: ReactNode;
  children: ReactNode;
  defaultOpen?: boolean;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** 触发按钮的可访问名称。 */
  ariaLabel?: string;
}) {
  const [uncontrolled, setUncontrolled] = useState(defaultOpen);
  const open = controlledOpen ?? uncontrolled;
  const id = useId();
  const panelId = `${id}-panel`;
  const triggerId = `${id}-trigger`;

  const toggle = () => {
    const next = !open;
    if (controlledOpen === undefined) setUncontrolled(next);
    onOpenChange?.(next);
  };

  return (
    <DisclosureContext.Provider value={{ open, toggle, panelId, triggerId }}>
      <div className="disclosure" data-open={open || undefined}>
        <h3 className="disclosure-heading">
          <button
            type="button"
            id={triggerId}
            className="disclosure-trigger"
            aria-label={ariaLabel}
            aria-expanded={open}
            aria-controls={panelId}
            onClick={toggle}
          >
            <span className="disclosure-title">{title}</span>
            <span className="disclosure-indicator" data-open={open || undefined} aria-hidden="true">
              ⌄
            </span>
          </button>
        </h3>
        {/*
          收起时不渲染面板内容，避免读屏读到不可见内容。
          这一点很重要：面板里常放表格等重内容，留在无障碍树里会让读屏
          读到不可见内容，也让「展开后才出现」的断言失效。
        */}
        {open && (
          <div
            id={panelId}
            role="region"
            aria-labelledby={triggerId}
            className="disclosure-panel"
            data-open={open || undefined}
          >
            <div className="disclosure-content">{children}</div>
          </div>
        )}
      </div>
    </DisclosureContext.Provider>
  );
}
