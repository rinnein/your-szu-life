import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { BottomSheet, Dialog } from "material/react";
import { Chip, Kbd } from "../ui";
import { LuArrowRight, LuClock, LuFlame, LuSearch } from "react-icons/lu";
import { useLogicalNavigation } from "../logical-navigation";
import { COMMAND_LIST, filterCommands } from "./commands";
import { getFrequentCommands, getRecentCommands, recordCommandUsage } from "./history";
import type { CommandItem } from "./types";

export function openCommandPalette() {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent("open-command-palette"));
  }
}

export function CommandPalette() {
  const navigation = useLogicalNavigation();
  const [isOpen, setIsOpen] = useState(false);
  const [isMobile, setIsMobile] = useState(false);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const check = () => setIsMobile(window.innerWidth < 640);
    check();
    window.addEventListener("resize", check);
    return () => window.removeEventListener("resize", check);
  }, []);

  // Listen for global shortcut Ctrl+Shift+P (or Cmd+Shift+P) and custom event
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === "p") {
        e.preventDefault();
        setIsOpen((prev) => !prev);
      }
    };

    const handleOpenEvent = () => {
      setIsOpen(true);
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("open-command-palette", handleOpenEvent);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("open-command-palette", handleOpenEvent);
    };
  }, []);

  // Reset state when opening/closing
  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  // Compute displayed commands
  const recentCommands = useMemo(() => getRecentCommands(COMMAND_LIST, 3), [isOpen]);
  const frequentCommands = useMemo(() => getFrequentCommands(COMMAND_LIST, 3), [isOpen]);

  const displayedCommands: CommandItem[] = useMemo(() => {
    const trimmed = query.trim();
    if (trimmed) {
      return filterCommands(trimmed, COMMAND_LIST);
    }
    // When query is empty, recommend recent & frequent, then remaining commands
    const seen = new Set<string>();
    const list: CommandItem[] = [];

    for (const c of recentCommands) {
      if (!seen.has(c.id)) {
        seen.add(c.id);
        list.push(c);
      }
    }
    for (const c of frequentCommands) {
      if (!seen.has(c.id)) {
        seen.add(c.id);
        list.push(c);
      }
    }
    for (const c of COMMAND_LIST) {
      if (!seen.has(c.id)) {
        seen.add(c.id);
        list.push(c);
      }
    }
    return list;
  }, [query, recentCommands, frequentCommands]);

  // Keep selected index within bounds
  useEffect(() => {
    setSelectedIndex((current) => {
      if (displayedCommands.length === 0) return 0;
      return Math.min(current, displayedCommands.length - 1);
    });
  }, [displayedCommands.length]);

  // Scroll active item into view
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const activeEl = list.querySelector<HTMLElement>(`[data-index="${selectedIndex}"]`);
    activeEl?.scrollIntoView?.({ block: "nearest" });
  }, [selectedIndex]);

  const executeCommand = async (command: CommandItem) => {
    recordCommandUsage(command.id);
    setIsOpen(false);
    const targetUrl =
      typeof command.target === "function" ? await command.target() : command.target;
    void navigation.push(targetUrl);
  };

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((idx) =>
        displayedCommands.length > 0 ? (idx + 1) % displayedCommands.length : 0,
      );
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((idx) =>
        displayedCommands.length > 0
          ? (idx - 1 + displayedCommands.length) % displayedCommands.length
          : 0,
      );
    } else if (e.key === "Enter") {
      e.preventDefault();
      const current = displayedCommands[selectedIndex];
      if (current) {
        void executeCommand(current);
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      setIsOpen(false);
    }
  };

  if (!isOpen) return null;

  const content = (
    <div className="overflow-hidden p-0 w-full">
      {/* Header Search Bar */}
      <div className="flex items-center gap-3 border-b border-default-200 px-4 py-3">
        <LuSearch className="size-5 text-muted shrink-0" aria-hidden="true" />
        <input
          ref={inputRef}
          id="command-palette-search"
          aria-label="命令面板搜索框"
          type="text"
          className="flex-1 bg-transparent text-base outline-none placeholder:text-muted"
          placeholder="输入功能名称或快捷缩写"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSelectedIndex(0);
          }}
          onKeyDown={handleInputKeyDown}
        />
        <div className="flex items-center gap-1">
          <Kbd>Esc</Kbd>
        </div>
      </div>

      {/* Results List */}
      <div
        ref={listRef}
        className="max-h-80 overflow-y-auto p-2"
        role="listbox"
        aria-label="命令选项"
      >
        {displayedCommands.length === 0 ? (
          <div className="py-8 text-center text-sm text-muted">未找到匹配的快捷指令或功能</div>
        ) : (
          displayedCommands.map((command, idx) => {
            const isSelected = idx === selectedIndex;
            const Icon = command.icon;
            const isRecent = recentCommands.some((r) => r.id === command.id);
            const isFrequent = frequentCommands.some((f) => f.id === command.id);

            return (
              <button
                key={command.id}
                type="button"
                data-index={idx}
                role="option"
                aria-selected={isSelected}
                className={`group relative flex w-full items-center justify-between gap-3 rounded-xl p-2.5 text-left text-sm transition-all border ${
                  isSelected
                    ? "bg-primary/10 border-primary/50 shadow-sm text-foreground ring-1 ring-primary/30"
                    : "border-transparent hover:bg-default-100/70 text-foreground/80"
                }`}
                onClick={() => void executeCommand(command)}
                onMouseEnter={() => setSelectedIndex(idx)}
              >
                <div className="flex items-center gap-2.5 min-w-0 flex-1">
                  {/* 选中标记 Indicator bar */}
                  <span
                    className={`w-1.5 h-8 rounded-full shrink-0 transition-all ${
                      isSelected
                        ? "bg-primary scale-100 opacity-100"
                        : "bg-transparent scale-50 opacity-0"
                    }`}
                    aria-hidden="true"
                  />
                  {/* 左侧图标变色 */}
                  <div
                    className={`flex size-9 shrink-0 items-center justify-center rounded-xl transition-all ${
                      isSelected
                        ? "bg-primary text-primary-foreground shadow-md shadow-primary/25 scale-105"
                        : "bg-default-100 text-default-500"
                    }`}
                  >
                    {Icon ? <Icon className="size-4" /> : <LuSearch className="size-4" />}
                  </div>
                  {/* 左侧标题与描述变色 */}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span
                        className={`transition-colors ${
                          isSelected
                            ? "font-bold text-primary text-[15px]"
                            : "font-medium text-foreground text-[14px]"
                        }`}
                      >
                        {command.name}
                      </span>
                      {!query && isRecent && (
                        <span
                          className={`flex items-center gap-0.5 text-[10px] font-normal transition-colors ${
                            isSelected ? "text-primary/75" : "text-muted"
                          }`}
                        >
                          <LuClock className="size-3" aria-hidden="true" />
                          最近
                        </span>
                      )}
                      {!query && isFrequent && (
                        <span className="flex items-center gap-0.5 text-[10px] text-warning font-normal">
                          <LuFlame className="size-3" aria-hidden="true" />
                          常用
                        </span>
                      )}
                    </div>
                    <div
                      className={`truncate text-xs font-normal transition-colors ${
                        isSelected ? "text-primary/75" : "text-muted"
                      }`}
                    >
                      {command.description}
                    </div>
                  </div>
                </div>

                <div className="flex items-center gap-2 shrink-0">
                  <div className="flex items-center gap-1">
                    {command.shortcuts.map((shortcut) => (
                      <Chip
                        key={shortcut}
                        size="sm"
                        variant={isSelected ? "primary" : "soft"}
                        color={isSelected ? "accent" : "default"}
                      >
                        {shortcut}
                      </Chip>
                    ))}
                  </div>
                  <LuArrowRight
                    className={`size-4 transition-all ${
                      isSelected
                        ? "text-primary opacity-100 translate-x-0"
                        : "text-default-400 opacity-0 -translate-x-1"
                    }`}
                    aria-hidden="true"
                  />
                </div>
              </button>
            );
          })
        )}
      </div>

      {/* Footer Shortcuts Hint */}
      <div className="flex flex-wrap items-center justify-between border-t border-default-200 px-4 py-2 text-xs text-muted">
        <div className="flex items-center gap-2">
          <span>
            <Kbd>↑↓</Kbd> 切换
          </span>
          <span>
            <Kbd>Enter</Kbd> 打开
          </span>
        </div>
        <div className="flex items-center gap-1">
          <span>按</span>
          <Kbd>⌘ ⇧ P</Kbd>
          <span>随处唤起</span>
        </div>
      </div>
    </div>
  );

  if (typeof document === "undefined") return null;

  if (isMobile) {
    return createPortal(
      <BottomSheet modal open={isOpen} headline="快捷搜索" onClose={() => setIsOpen(false)}>
        {content}
      </BottomSheet>,
      document.body,
    );
  }

  return createPortal(
    <Dialog
      open={isOpen}
      onClose={() => setIsOpen(false)}
      headline="快捷搜索"
      className="p-0 sm:max-w-xl"
    >
      {content}
    </Dialog>,
    document.body,
  );
}
