import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Drawer, DrawerItem } from "material/react";

export interface AdaptiveTabItem {
  value: string;
  label: string;
  icon?: ReactNode | string;
  badge?: string | number;
  disabled?: boolean;
}

export interface DrawerSection {
  id: string;
  label?: string;
  items: readonly AdaptiveTabItem[];
  value: string;
  onChange: (value: string) => void;
}

interface PageDrawerActionsContextValue {
  setOpen: (open: boolean) => void;
  registerSection: (section: DrawerSection) => () => void;
}

interface PageDrawerStateContextValue {
  sections: readonly DrawerSection[];
  open: boolean;
}

const PageDrawerActionsContext = createContext<PageDrawerActionsContextValue | null>(null);
const PageDrawerStateContext = createContext<PageDrawerStateContextValue | null>(null);

/** 供 AdaptiveTabs 等注册方使用：函数引用完全稳定，注册或状态变更绝不会诱发调用方重渲染 */
export function usePageDrawerActions() {
  return useContext(PageDrawerActionsContext);
}

/** 供 TopAppBar 等观察方使用：获取当前已注册 sections 与 open 状态 */
export function usePageDrawerState() {
  return useContext(PageDrawerStateContext);
}

/** 兼容性聚合 Hook */
export function usePageDrawer() {
  const actions = usePageDrawerActions();
  const state = usePageDrawerState();
  if (!actions || !state) return null;
  return {
    ...state,
    ...actions,
  };
}

export function PageDrawerProvider({ children }: { children: ReactNode }) {
  const [sectionsMap, setSectionsMap] = useState<Map<string, DrawerSection>>(() => new Map());
  const [open, setOpen] = useState(false);

  const registerSection = useCallback((section: DrawerSection) => {
    setSectionsMap((prev) => {
      const next = new Map(prev);
      next.set(section.id, section);
      return next;
    });
    return () => {
      setSectionsMap((prev) => {
        if (!prev.has(section.id)) return prev;
        const next = new Map(prev);
        next.delete(section.id);
        return next;
      });
    };
  }, []);

  const actionsValue = useMemo<PageDrawerActionsContextValue>(
    () => ({
      setOpen,
      registerSection,
    }),
    [registerSection],
  );

  const sections = useMemo(() => Array.from(sectionsMap.values()), [sectionsMap]);

  const stateValue = useMemo<PageDrawerStateContextValue>(
    () => ({
      sections,
      open,
    }),
    [sections, open],
  );

  // 计算当前激活项的全局唯一 composite value
  const activeCompositeValue = useMemo(() => {
    for (const section of sections) {
      if (section.value) {
        return `${section.id}::${section.value}`;
      }
    }
    return undefined;
  }, [sections]);

  const handleDrawerChange = useCallback(
    (event: CustomEvent<{ value: string }>) => {
      const raw = event.detail?.value;
      if (!raw) return;
      const separatorIdx = raw.indexOf("::");
      if (separatorIdx === -1) {
        for (const section of sections) {
          const match = section.items.find((item) => item.value === raw);
          if (match) {
            setOpen(false);
            section.onChange(match.value);
            return;
          }
        }
        return;
      }

      const sectionId = raw.slice(0, separatorIdx);
      const itemValue = raw.slice(separatorIdx + 2);
      const targetSection = sections.find((s) => s.id === sectionId);
      if (targetSection) {
        setOpen(false);
        targetSection.onChange(itemValue);
      }
    },
    [sections],
  );

  return (
    <PageDrawerActionsContext.Provider value={actionsValue}>
      <PageDrawerStateContext.Provider value={stateValue}>
        {children}
        {sections.length > 0 &&
          typeof document !== "undefined" &&
          createPortal(
            <Drawer
              modal
              open={open}
              position="start"
              width="280"
              headline="页面导航"
              value={activeCompositeValue}
              onChange={handleDrawerChange as unknown as (event: Event) => void}
              onClose={() => setOpen(false)}
            >
              {sections.map((section, idx) => (
                <DrawerSectionElements
                  key={section.id}
                  section={section}
                  showDivider={idx > 0}
                  showSectionLabel={Boolean(section.label)}
                />
              ))}
            </Drawer>,
            document.body,
          )}
      </PageDrawerStateContext.Provider>
    </PageDrawerActionsContext.Provider>
  );
}

function DrawerSectionElements({
  section,
  showDivider,
  showSectionLabel,
}: {
  section: DrawerSection;
  showDivider: boolean;
  showSectionLabel: boolean;
}) {
  return (
    <>
      {showDivider && <DrawerItem type="divider" />}
      {showSectionLabel && section.label && <DrawerItem type="section" label={section.label} />}
      {section.items.map((item) => (
        <DrawerItem
          key={item.value}
          value={`${section.id}::${item.value}`}
          label={item.label}
          icon={typeof item.icon === "string" ? item.icon : undefined}
          badge={item.badge !== undefined ? String(item.badge) : undefined}
          disabled={item.disabled}
        />
      ))}
    </>
  );
}
