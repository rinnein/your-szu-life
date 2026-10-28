import type { Contest, Group } from "./types";

/**
 * 班级/测验列表的进程内缓存。
 *
 * 后端已经会把列表持久化到 SQLite，并支持 stale-while-error；这里再缓存一层，
 * 让页面在站点/班级之间来回切换时不必每次都发一次 IPC，也不会先清空列表再
 * 渲染出“暂无可见的班级/测验”的空状态。
 */
export const LIST_TTL_MS = 10 * 60_000;

export interface ListCacheEntry<T> {
  rows: T[];
  updatedAt: number;
}

const groups = new Map<string, ListCacheEntry<Group>>();
const contests = new Map<string, ListCacheEntry<Contest>>();

export const contestKey = (siteId: string, groupId: string) => `${siteId}:${groupId}`;

function fresh<T>(entry: ListCacheEntry<T> | undefined, now: number) {
  return entry && now - entry.updatedAt <= LIST_TTL_MS ? entry : undefined;
}

export function readGroups(siteId: string, now = Date.now()): ListCacheEntry<Group> | undefined {
  return fresh(groups.get(siteId), now);
}

export function writeGroups(siteId: string, rows: Group[], now = Date.now()): void {
  groups.set(siteId, { rows, updatedAt: now });
}

export function readContests(
  siteId: string,
  groupId: string,
  now = Date.now(),
): ListCacheEntry<Contest> | undefined {
  return fresh(contests.get(contestKey(siteId, groupId)), now);
}

export function writeContests(
  siteId: string,
  groupId: string,
  rows: Contest[],
  now = Date.now(),
): void {
  contests.set(contestKey(siteId, groupId), { rows, updatedAt: now });
}

export function clearListCache(): void {
  groups.clear();
  contests.clear();
}
