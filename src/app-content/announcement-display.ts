const SEEN_IDS_KEY = "announcement_popup_seen_ids";
const MAX_SEEN_IDS = 200;

/** popup 公告同一个 id 只弹一次，「弹过」状态记录在前端 localStorage。 */
export function isAnnouncementPopupSeen(id: string): boolean {
  try {
    const raw = localStorage.getItem(SEEN_IDS_KEY);
    if (!raw) return false;
    const ids: unknown = JSON.parse(raw);
    return Array.isArray(ids) && ids.includes(id);
  } catch {
    return false;
  }
}

export function markAnnouncementPopupSeen(id: string): void {
  try {
    const raw = localStorage.getItem(SEEN_IDS_KEY);
    if (!raw) {
      localStorage.setItem(SEEN_IDS_KEY, JSON.stringify([id]));
      return;
    }
    const ids: unknown = JSON.parse(raw);
    if (!Array.isArray(ids)) return;
    if (!ids.includes(id)) {
      // 只保留最近 MAX_SEEN_IDS 条 id，避免记录无限增长
      ids.push(id);
      localStorage.setItem(SEEN_IDS_KEY, JSON.stringify(ids.slice(-MAX_SEEN_IDS)));
    }
  } catch {
    // Ignore storage quota or disabled localStorage exceptions
  }
}
