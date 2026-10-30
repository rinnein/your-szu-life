import { invoke } from "@tauri-apps/api/core";
import { route, type Notice, type Snapshot } from "./types";

/** 通知 extra 里的 `ojNotice` 是不可信的外部输入，先做形状检查。 */
export function isOjNotice(value: unknown): value is Notice {
  if (!value || typeof value !== "object") return false;
  const notice = value as Partial<Notice>;
  return (
    typeof notice.siteId === "string" &&
    typeof notice.account === "string" &&
    typeof notice.contestId === "string" &&
    (notice.contestId === "" || /^\d+$/.test(notice.contestId))
  );
}

/**
 * 把通知携带的 OJ 测验解析成可导航路由。
 *
 * 系统通知是提前排程的，用户点开时站点可能已被删除、禁用或换号，
 * 因此先按当前快照核对再返回路由；核对失败就不跳转。
 */
export async function resolveOjNoticeRoute(notice: Notice): Promise<string | undefined> {
  try {
    const snapshot = await invoke<Snapshot>("get_oj_snapshot");
    const enabled = snapshot.sites.some(
      (site) => site.id === notice.siteId && site.username === notice.account && site.enabled,
    );
    if (!enabled) return undefined;
    return route({ site: notice.siteId, contest: notice.contestId });
  } catch {
    return undefined;
  }
}
