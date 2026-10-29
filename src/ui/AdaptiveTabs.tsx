import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { Tabs, Tab } from "material/react";
import {
  LuCheck,
  LuChevronDown,
  LuChevronLeft,
  LuChevronRight,
  LuSearch,
  LuX,
} from "react-icons/lu";
import { usePageDrawerActions, type AdaptiveTabItem } from "../shell/PageDrawerContext";
import { useInlineSizeAtLeast } from "../responsive";
import { nameTablist } from "./tablist";

export type { AdaptiveTabItem };

export interface AdaptiveTabsProps {
  items: readonly AdaptiveTabItem[];
  value: string;
  onChange: (value: string) => void;
  /** 外观变体：standard（默认 Material 下划线标签）、segmented（分段选择器）、pill（独立药丸胶囊） */
  variant?: "standard" | "segmented" | "pill";
  /** 尺寸：sm（32px紧凑工具栏尺寸）、md（40px标准尺寸） */
  size?: "sm" | "md";
  /** 下拉模式是否启用搜索过滤（默认在项数 > 8 时自动开启） */
  searchable?: boolean;
  /** 抽屉与折叠下拉中展示的分组标题 */
  label?: string;
  /** 是否向左上角抽屉自动注册该项导航，默认为 true */
  injectDrawer?: boolean;
  /** 单个 Tab 的预估宽度（px），若未传入则依据选项文字长度与图标动态精准计算 */
  estimatedItemWidth?: number;
  /** 固定最小宽度阈值（若传入则优先按此数值判断） */
  minWidthForTabs?: number;
  ariaLabel?: string;
  className?: string;
}

export function renderAdaptiveIcon(
  icon: ReactNode | string | undefined,
  className = "",
): ReactNode {
  if (!icon) return null;
  if (typeof icon === "string") {
    const trimmed = icon.trim();
    if (trimmed.startsWith("<") && trimmed.endsWith(">")) {
      return (
        <span
          className={`adaptive-tabs-icon ${className}`}
          dangerouslySetInnerHTML={{ __html: icon }}
        />
      );
    }
    return <span className={`adaptive-tabs-icon ${className}`}>{icon}</span>;
  }
  return <span className={`adaptive-tabs-icon ${className}`}>{icon}</span>;
}

/**
 * 依据选项内容（字数、中英文比例、图标与徽标）计算 Tabs 真正横向容纳所需像素宽度。
 * 彻底避免固定预估宽度在多题目或长标题下误判空间充足导致横向截断。
 */
export function estimateRequiredTabsWidth(
  items: readonly AdaptiveTabItem[],
  variant: "standard" | "segmented" | "pill" = "standard",
  explicitItemWidth?: number,
): number {
  if (items.length === 0) return 0;
  if (explicitItemWidth && explicitItemWidth !== 115) return items.length * explicitItemWidth;

  let total = 0;
  for (const item of items) {
    // 字符数加权：中文字符/全角字符占 15px，英文/半角字符占 8.5px
    let charWidth = 0;
    for (let i = 0; i < item.label.length; i++) {
      charWidth += item.label.charCodeAt(i) > 255 ? 15 : 8.5;
    }
    // 图标宽度（判题彩色胶囊约 46px，普通图标约 24px）
    const iconWidth = item.icon
      ? typeof item.icon === "string" && item.icon.includes("<")
        ? 46
        : 24
      : 0;
    // 徽标宽度
    const badgeWidth = item.badge !== undefined ? 28 : 0;
    // 内边距与左右间距（segmented 控制器紧凑，standard tab 宽松）
    const padding = variant === "segmented" ? 16 : variant === "pill" ? 24 : 38;
    const minItem = variant === "segmented" ? 48 : variant === "pill" ? 64 : 80;

    total += Math.max(charWidth + iconWidth + badgeWidth + padding, minItem);
  }

  // 额外缓冲与容器边距
  return total + (variant === "segmented" ? 8 : 16);
}

export function AdaptiveTabs({
  items,
  value,
  onChange,
  variant = "standard",
  size = "md",
  searchable,
  label,
  injectDrawer = true,
  estimatedItemWidth = 115,
  minWidthForTabs,
  ariaLabel,
  className = "",
}: AdaptiveTabsProps) {
  const drawerActions = usePageDrawerActions();
  const sectionId = useId();

  // 1. 自动注入左上角抽屉（如果启用）
  useEffect(() => {
    if (!injectDrawer || !drawerActions) return;
    return drawerActions.registerSection({
      id: sectionId,
      label,
      items,
      value,
      onChange,
    });
  }, [injectDrawer, drawerActions, sectionId, label, items, value, onChange]);

  // 2. 空间预算判定：计算所需宽度（无硬编码 220px 下限，让小尺寸 segmented/pill 能自然在 180px 展开）
  const threshold =
    minWidthForTabs ??
    estimateRequiredTabsWidth(
      items,
      variant,
      estimatedItemWidth === 115 ? undefined : estimatedItemWidth,
    );
  const { ref: containerRef, reached: isWide } = useInlineSizeAtLeast(threshold);

  const [menuOpen, setMenuOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [highlightIndex, setHighlightIndex] = useState(-1);

  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(false);

  const stripRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  // 3. 宽模式下的横向溢出检测与左右平滑滚动支持
  const checkScroll = useCallback(() => {
    const el = stripRef.current;
    if (!el) return;
    const { scrollLeft, scrollWidth, clientWidth } = el;
    setCanScrollLeft(scrollLeft > 4);
    setCanScrollRight(scrollLeft + clientWidth < scrollWidth - 4);
  }, []);

  useEffect(() => {
    const el = stripRef.current;
    if (!el || !isWide) return;
    checkScroll();
    el.addEventListener("scroll", checkScroll, { passive: true });
    window.addEventListener("resize", checkScroll, { passive: true });
    return () => {
      el.removeEventListener("scroll", checkScroll);
      window.removeEventListener("resize", checkScroll);
    };
  }, [isWide, items, checkScroll]);

  const scrollByAmount = (amount: number) => {
    stripRef.current?.scrollBy({ left: amount, behavior: "smooth" });
  };

  useEffect(() => {
    const el = stripRef.current;
    if (!el || !isWide) return;
    const handleWheel = (e: WheelEvent) => {
      if (e.deltaY !== 0 && e.deltaX === 0) {
        e.preventDefault();
        const delta = e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY;
        el.scrollLeft += delta;
      }
    };
    el.addEventListener("wheel", handleWheel, { passive: false });
    return () => el.removeEventListener("wheel", handleWheel);
  }, [isWide]);

  useEffect(() => {
    if (!value || !isWide) return;
    const el = stripRef.current;
    if (!el) return;
    const activeTab =
      el.querySelector<HTMLElement>(`m-tab[value="${value}"]`) ||
      el.querySelector<HTMLElement>(`[data-tab-value="${value}"]`);
    if (!activeTab) return;

    // 仅在 tab 横向滚动容器内部计算平滑居中，直接操作容器自身的 scrollLeft / scrollTo，
    // 严禁调用 Element.prototype.scrollIntoView：DOM 规范中该 API 会向上穿透滚动全局 window，
    // 当页面滚动在下方时，会强行将整个页面滚回顶部。
    const stripRect = el.getBoundingClientRect();
    const tabRect = activeTab.getBoundingClientRect();
    if (stripRect.width > 0 && tabRect.width > 0) {
      const offset = el.scrollLeft + (tabRect.left - stripRect.left);
      const targetLeft = Math.max(0, offset - (stripRect.width - tabRect.width) / 2);
      if (typeof el.scrollTo === "function") {
        el.scrollTo({ left: targetLeft, behavior: "smooth" });
      } else {
        el.scrollLeft = targetLeft;
      }
    }
  }, [value, isWide]);

  // 4. 窄模式浮层外部点击与 ESC 自动收起
  useEffect(() => {
    if (!menuOpen) {
      setSearchQuery("");
      setHighlightIndex(-1);
      return;
    }

    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (triggerRef.current?.contains(target) || popoverRef.current?.contains(target)) {
        return;
      }
      setMenuOpen(false);
    };

    const handleKeyDown = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") {
        setMenuOpen(false);
        triggerRef.current?.focus({ preventScroll: true });
      }
    };

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [menuOpen]);

  const currentItem = items.find((i) => i.value === value) ?? items[0];

  const handleCombinedRef = (node: HTMLDivElement | null) => {
    stripRef.current = node;
    containerRef(node);
  };

  const isSearchEnabled = searchable ?? items.length > 8;
  const filteredItems =
    isSearchEnabled && searchQuery.trim()
      ? items.filter((item) => item.label.toLowerCase().includes(searchQuery.trim().toLowerCase()))
      : items;

  const handlePopoverKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlightIndex((prev) => (prev < filteredItems.length - 1 ? prev + 1 : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlightIndex((prev) => (prev > 0 ? prev - 1 : filteredItems.length - 1));
    } else if (e.key === "Enter" && highlightIndex >= 0 && highlightIndex < filteredItems.length) {
      e.preventDefault();
      const target = filteredItems[highlightIndex];
      if (target && !target.disabled) {
        setMenuOpen(false);
        onChange(target.value);
        triggerRef.current?.focus({ preventScroll: true });
      }
    }
  };

  // 宽模式渲染（带有左右滚动指示箭头的横条）
  if (isWide) {
    let content: ReactNode;

    if (variant === "segmented") {
      content = (
        <div
          ref={handleCombinedRef}
          className={`adaptive-tabs adaptive-tabs-strip adaptive-tabs-segmented adaptive-tabs--${size} ${className}`}
          role="tablist"
          aria-label={ariaLabel ?? label}
          data-adaptive-mode="segmented"
          data-adaptive-variant={variant}
          data-adaptive-size={size}
        >
          {items.map((item) => {
            const isSelected = item.value === value;
            return (
              <button
                key={item.value}
                type="button"
                role="tab"
                data-tab-value={item.value}
                aria-selected={isSelected}
                disabled={item.disabled}
                className={`adaptive-tabs-segmented-btn ${
                  isSelected ? "adaptive-tabs-segmented-btn--selected" : ""
                }`}
                onClick={() => onChange(item.value)}
              >
                {renderAdaptiveIcon(item.icon, "adaptive-tabs-segmented-icon")}
                <span className="adaptive-tabs-segmented-label">{item.label}</span>
                {item.badge !== undefined && (
                  <span className="adaptive-tabs-item-badge">{item.badge}</span>
                )}
              </button>
            );
          })}
        </div>
      );
    } else if (variant === "pill") {
      content = (
        <div
          ref={handleCombinedRef}
          className={`adaptive-tabs adaptive-tabs-strip adaptive-tabs-pills adaptive-tabs--${size} ${className}`}
          role="tablist"
          aria-label={ariaLabel ?? label}
          data-adaptive-mode="pill"
          data-adaptive-variant={variant}
          data-adaptive-size={size}
        >
          {items.map((item) => {
            const isSelected = item.value === value;
            return (
              <button
                key={item.value}
                type="button"
                role="tab"
                data-tab-value={item.value}
                aria-selected={isSelected}
                disabled={item.disabled}
                className={`adaptive-tabs-pill ${isSelected ? "adaptive-tabs-pill--selected" : ""}`}
                onClick={() => onChange(item.value)}
              >
                {renderAdaptiveIcon(item.icon, "adaptive-tabs-pill-icon")}
                <span className="adaptive-tabs-pill-label">{item.label}</span>
                {item.badge !== undefined && (
                  <span className="adaptive-tabs-item-badge">{item.badge}</span>
                )}
              </button>
            );
          })}
        </div>
      );
    } else {
      content = (
        <div
          ref={handleCombinedRef}
          className={`adaptive-tabs adaptive-tabs-strip oj-switcher-strip adaptive-tabs--${size} ${className}`}
          data-adaptive-mode="tabs"
          data-adaptive-variant={variant}
          data-adaptive-size={size}
        >
          <Tabs
            ref={(host) => {
              if (host && ariaLabel) nameTablist(host, ariaLabel);
            }}
            value={value}
            onChange={(event) => {
              const nextValue = event.detail?.value;
              if (nextValue !== undefined && nextValue !== null) {
                onChange(nextValue);
              }
            }}
          >
            {items.map((item) => (
              <Tab
                key={item.value}
                value={item.value}
                icon={typeof item.icon === "string" ? item.icon : undefined}
                badge={item.badge !== undefined ? String(item.badge) : undefined}
                disabled={item.disabled}
              >
                {item.label}
              </Tab>
            ))}
          </Tabs>
        </div>
      );
    }

    return (
      <div className="adaptive-tabs-strip-container relative flex items-center w-full min-w-0">
        {canScrollLeft && (
          <button
            type="button"
            className="adaptive-tabs-scroll-btn adaptive-tabs-scroll-btn--left"
            aria-label="向左滚动标签"
            onClick={() => scrollByAmount(-220)}
          >
            <LuChevronLeft aria-hidden="true" />
          </button>
        )}
        {content}
        {canScrollRight && (
          <button
            type="button"
            className="adaptive-tabs-scroll-btn adaptive-tabs-scroll-btn--right"
            aria-label="向右滚动标签"
            onClick={() => scrollByAmount(220)}
          >
            <LuChevronRight aria-hidden="true" />
          </button>
        )}
      </div>
    );
  }

  // 窄模式（折叠下拉）：纯 CSS 相对容器绝对定位，零延迟、零闪烁、带丝滑展开动画
  return (
    <div
      ref={containerRef}
      className={`adaptive-tabs adaptive-tabs-container adaptive-tabs--${size} relative ${className}`}
      data-adaptive-mode="dropdown"
      data-adaptive-variant={variant}
      data-adaptive-size={size}
    >
      <button
        type="button"
        ref={triggerRef}
        className={`adaptive-tabs-trigger adaptive-tabs-trigger--${size} adaptive-tabs-trigger--${variant}`}
        aria-haspopup="listbox"
        aria-expanded={menuOpen}
        aria-label={ariaLabel ?? (label ? `${label}：${currentItem?.label ?? value}` : undefined)}
        onClick={() => setMenuOpen((open) => !open)}
      >
        {currentItem?.icon && (
          <span className="adaptive-tabs-trigger-icon">{renderAdaptiveIcon(currentItem.icon)}</span>
        )}
        <span className="adaptive-tabs-trigger-label">
          {label ? `${label}：` : ""}
          {currentItem?.label ?? value}
        </span>
        <LuChevronDown
          aria-hidden="true"
          className={`adaptive-tabs-trigger-chevron transition-transform duration-200 ${
            menuOpen ? "rotate-180" : ""
          }`}
        />
      </button>

      {menuOpen && (
        <div
          ref={popoverRef}
          className="adaptive-tabs-popover"
          role="listbox"
          aria-label={ariaLabel ?? label ?? "选项列表"}
          onKeyDown={handlePopoverKeyDown}
        >
          {isSearchEnabled && (
            <div className="adaptive-tabs-search-box">
              <LuSearch className="adaptive-tabs-search-icon" aria-hidden="true" />
              <input
                ref={searchInputRef}
                type="text"
                className="adaptive-tabs-search-input"
                placeholder={`查找${label || "选项"}…`}
                value={searchQuery}
                onChange={(e) => {
                  setSearchQuery(e.target.value);
                  setHighlightIndex(0);
                }}
                autoFocus
                onClick={(e) => e.stopPropagation()}
              />
              {searchQuery && (
                <button
                  type="button"
                  className="adaptive-tabs-search-clear"
                  aria-label="清空搜索"
                  onClick={(e) => {
                    e.stopPropagation();
                    setSearchQuery("");
                    searchInputRef.current?.focus();
                  }}
                >
                  <LuX aria-hidden="true" />
                </button>
              )}
            </div>
          )}

          <div className="adaptive-tabs-options-list">
            {filteredItems.length === 0 ? (
              <div className="adaptive-tabs-empty">没有匹配的选项</div>
            ) : (
              filteredItems.map((item, idx) => {
                const isSelected = item.value === value;
                const isHighlighted = idx === highlightIndex;
                return (
                  <button
                    key={item.value}
                    type="button"
                    role="option"
                    aria-selected={isSelected}
                    disabled={item.disabled}
                    className={`adaptive-tabs-item ${
                      isSelected ? "adaptive-tabs-item--selected" : ""
                    } ${isHighlighted ? "adaptive-tabs-item--highlighted" : ""}`}
                    onClick={() => {
                      setMenuOpen(false);
                      onChange(item.value);
                    }}
                  >
                    {item.icon && (
                      <span className="adaptive-tabs-item-icon">
                        {renderAdaptiveIcon(item.icon)}
                      </span>
                    )}
                    <span className="adaptive-tabs-item-label">{item.label}</span>
                    {item.badge !== undefined && (
                      <span className="adaptive-tabs-item-badge">{item.badge}</span>
                    )}
                    {isSelected && (
                      <LuCheck aria-hidden="true" className="adaptive-tabs-item-check ml-auto" />
                    )}
                  </button>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}
