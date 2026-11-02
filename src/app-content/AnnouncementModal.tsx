import { useState } from "react";
import { createPortal } from "react-dom";
import { Button, Dialog } from "material/react";
import { LuArrowRight, LuArrowUpRight } from "react-icons/lu";
import { useLogicalNavigation } from "../logical-navigation";
import { openPublicUrl } from "./external";
import { AnnouncementImage, resolveAnnouncementLink } from "./AnnouncementBoard";
import type { Announcement } from "./types";

/**
 * 公告详情模态框：首页横条点击与全局 popup / alert 弹出共用同一个 UI。
 * 关闭时通过 onClose 交还控制权，弹出去重（popup 记 localStorage、alert 记会话）由调用方处理。
 */
export function AnnouncementModal({
  announcement,
  onClose,
}: {
  announcement: Announcement;
  onClose: () => void;
}) {
  const navigation = useLogicalNavigation();
  const [openError, setOpenError] = useState<string>();
  const linkTarget = announcement.goto ? resolveAnnouncementLink(announcement.goto) : null;

  const close = () => {
    setOpenError(undefined);
    onClose();
  };

  return createPortal(
    <Dialog
      open
      headline={announcement.title}
      role="dialog"
      aria-label={announcement.title}
      className="announcement-modal max-w-lg w-full"
      onClose={close}
      actions={
        <>
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
        </>
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
