import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Button, Dialog } from "material/react";
import { LuArrowLeft, LuArrowRight, LuArrowUpRight } from "react-icons/lu";
import { useLogicalNavigation } from "../logical-navigation";
import { openPublicUrl } from "./external";
import { AnnouncementImage, resolveAnnouncementLink } from "./AnnouncementBoard";
import type { Announcement } from "./types";

/**
 * 公告详情模态框：首页横条点击与全局 popup / alert 弹出共用同一个 UI。
 * 关闭时通过 onClose 交还控制权，弹出去重（popup 记 localStorage、alert 记会话）由调用方处理。
 *
 * 传入 index / total 与切换回调时，页脚会出现「上一条 / 下一条」浏览控件并支持 ← / → 键。
 * 只有首页公告板这么用：popup 队列是待弹公告的子集，不是可浏览的列表。
 */
export function AnnouncementModal({
  announcement,
  onClose,
  index,
  total,
  onPrevious,
  onNext,
}: {
  announcement: Announcement;
  onClose: () => void;
  index?: number;
  total?: number;
  onPrevious?: () => void;
  onNext?: () => void;
}) {
  const navigation = useLogicalNavigation();
  const [openError, setOpenError] = useState<string>();
  const linkTarget = announcement.goto ? resolveAnnouncementLink(announcement.goto) : null;
  const canBrowse =
    index !== undefined && (total ?? 0) > 1 && onPrevious !== undefined && onNext !== undefined;

  const close = () => {
    setOpenError(undefined);
    onClose();
  };

  // 翻到另一条公告后，上一条的「无法打开系统浏览器」提示不应残留。
  useEffect(() => {
    setOpenError(undefined);
  }, [announcement]);

  // ← / → 在模态框内翻看相邻公告；输入类控件与带修饰键的按键不拦截。
  useEffect(() => {
    if (!canBrowse) return;
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        target.closest("input, textarea, select, [contenteditable]")
      ) {
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        onPrevious?.();
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        onNext?.();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [canBrowse, onNext, onPrevious]);

  return createPortal(
    <Dialog
      open
      headline={announcement.title}
      role="dialog"
      aria-label={announcement.title}
      className="announcement-modal max-w-lg w-full"
      onClose={close}
      actions={
        <div className="flex w-full flex-wrap items-center gap-2">
          {canBrowse && (
            <div className="announcement-nav" role="group" aria-label="公告切换">
              <Button
                variant="text"
                className="announcement-nav-btn"
                aria-label="上一条公告"
                onClick={onPrevious}
              >
                <LuArrowLeft aria-hidden="true" />
              </Button>
              <span className="announcement-nav-index">
                {index + 1}/{total}
              </span>
              <Button
                variant="text"
                className="announcement-nav-btn"
                aria-label="下一条公告"
                onClick={onNext}
              >
                <LuArrowRight aria-hidden="true" />
              </Button>
            </div>
          )}
          <div className="ml-auto flex items-center gap-2">
            <Button variant="text" onClick={close}>
              关闭
            </Button>
            {linkTarget && (
              <Button
                variant="filled"
                onClick={() => {
                  if (linkTarget.kind === "internal") {
                    close();
                    void navigation.push(linkTarget.route);
                  } else {
                    setOpenError(undefined);
                    void openPublicUrl(linkTarget.url).catch(() =>
                      setOpenError("无法打开系统浏览器，请稍后重试。"),
                    );
                  }
                }}
              >
                {linkTarget.kind === "internal" ? "在软件内打开" : "在浏览器打开"}
                {linkTarget.kind === "internal" ? (
                  <LuArrowRight className="ml-1" aria-hidden="true" />
                ) : (
                  <LuArrowUpRight className="ml-1" aria-hidden="true" />
                )}
              </Button>
            )}
          </div>
        </div>
      }
    >
      <div className="flex flex-col gap-4 max-h-[70vh] overflow-y-auto pr-1">
        {/* 图片区域（可选） */}
        {announcement.pic && (
          <div className="rounded-lg overflow-hidden border border-outline-variant/60 w-full aspect-[16/9] bg-surface-container-high">
            <AnnouncementImage url={announcement.pic} title={announcement.title} />
          </div>
        )}

        {/* 描述文本（可选） */}
        {announcement.desc && (
          <div className="announcement-modal-desc text-sm text-foreground leading-relaxed whitespace-pre-wrap">
            {announcement.desc}
          </div>
        )}

        {openError && (
          <p className="announcement-open-error text-error text-xs" role="alert">
            {openError}
          </p>
        )}
      </div>
    </Dialog>,
    document.body,
  );
}
