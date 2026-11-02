import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { isChildDocument } from "../logical-navigation";
import { useAppContent } from "./context";
import { AnnouncementModal } from "./AnnouncementModal";
import { isAnnouncementPopupSeen, markAnnouncementPopupSeen } from "./announcement-display";
import type { Announcement } from "./types";

/**
 * 公告 popup / alert 模式的唯一弹窗出口，与 UpdateNotice 并列挂在根布局，
 * 无论用户当前在哪个页面都能弹出。
 *
 * - popup：携带 id 的定向公告，同一个 id 只弹一次，「弹过」记录在前端 localStorage；
 * - alert：每次冷启动弹一次，同一文档会话内按标题去重，不落 localStorage。
 *
 * 检测时机是公告快照到达或刷新：新出现的未弹 popup id 会被立即加入队列，
 * 多条公告按公告数组顺序逐条弹出。
 */
const alertedInSession = new Set<string>();

export function resetAnnouncementAlertsForTesting(): void {
  alertedInSession.clear();
}

function popupKey(unit: Announcement): string {
  return unit.mode === "popup" ? `id:${unit.id}` : `title:${unit.title}`;
}

export function AnnouncementPopupBridge() {
  const { snapshot } = useAppContent();
  const [queue, setQueue] = useState<Announcement[]>([]);

  const units = snapshot?.announcements.data?.announcements;

  useEffect(() => {
    if (!units?.length) return;
    if (typeof window === "undefined" || isChildDocument()) return;

    const incoming = units.filter(
      (unit) =>
        (unit.mode === "popup" && unit.id && !isAnnouncementPopupSeen(unit.id)) ||
        (unit.mode === "alert" && !alertedInSession.has(unit.title)),
    );
    if (incoming.length === 0) return;

    setQueue((current) => {
      const queued = new Set(current.map(popupKey));
      const next = [...current];
      for (const unit of incoming) {
        const key = popupKey(unit);
        if (!queued.has(key)) {
          queued.add(key);
          next.push(unit);
        }
      }
      return next;
    });
  }, [units]);

  const current = queue[0];
  if (!current) return null;

  const dismiss = () => {
    if (current.mode === "popup" && current.id) {
      markAnnouncementPopupSeen(current.id);
    } else if (current.mode === "alert") {
      alertedInSession.add(current.title);
    }
    setQueue((pending) => pending.slice(1));
  };

  return createPortal(
    <AnnouncementModal announcement={current} onClose={dismiss} />,
    document.body,
  );
}
