/**
 * 助教列表的前端 TTL 缓存。
 *
 * 后端没有任何 TA 列表缓存；是否刷新完全由这里决定：只有进入工作台/批阅列表
 * 且缓存超过 TTL 时才会 invoke，未过期直接复用。手动刷新无条件请求。
 * 不持久化、不跨账号、不跨会话 generation 复用。
 */
export const TA_LIST_TTL_MS = 5 * 60_000;

export interface TaCacheValue<T> {
  data: T;
  updatedAt: number;
}

const store = new Map<string, TaCacheValue<unknown>>();

export function taContestsKey(siteId: string, account: string, generation?: number | null) {
  return `contests:${siteId}:${account}:${generation ?? ""}`;
}

export function taReviewsKey(
  siteId: string,
  account: string,
  generation: number | null | undefined,
  contestId: string,
  query = "",
) {
  return `reviews:${siteId}:${account}:${generation ?? ""}:${contestId}:${query}`;
}

export function readTaCache<T>(key: string): (TaCacheValue<T> & { fresh: boolean }) | undefined {
  const hit = store.get(key) as TaCacheValue<T> | undefined;
  if (!hit) return undefined;
  return { ...hit, fresh: Date.now() - hit.updatedAt < TA_LIST_TTL_MS };
}

export function writeTaCache<T>(key: string, data: T, updatedAt = Date.now()) {
  store.set(key, { data, updatedAt });
}

export function invalidateTaCache(key: string) {
  store.delete(key);
}

export function invalidateTaReviews(
  siteId: string,
  account: string,
  generation: number | null | undefined,
  contestId: string,
) {
  const prefix = `reviews:${siteId}:${account}:${generation ?? ""}:${contestId}:`;
  for (const key of store.keys()) {
    if (key.startsWith(prefix)) store.delete(key);
  }
}

/** 测试与账号切换时清空全部 TA 列表缓存。 */
export function clearTaCache() {
  store.clear();
}
