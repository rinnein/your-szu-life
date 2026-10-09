import { createStore } from "@tanstack/react-store";
import { invoke } from "@tauri-apps/api/core";
import type { ProgressSummary, Site } from "./types";

export const PROGRESS_TTL = 5 * 60_000;
const RETRY_DELAY = 30_000;
export interface ProgressScope {
  siteId: string;
  account: string;
  url: string;
  generation: number | null;
}
interface Entry {
  value?: ProgressSummary;
  updatedAt: number;
  retryAfter: number;
  force: boolean;
  loading: boolean;
}
interface ScopeState {
  scope: ProgressScope;
  entries: Record<string, Entry>;
  revision: number;
}
export const progressScope = (site: Site): ProgressScope => ({
  siteId: site.id,
  account: site.username,
  url: site.url,
  generation: site.sessionGeneration ?? null,
});
export const progressScopeKey = (s: ProgressScope) =>
  JSON.stringify([s.siteId, s.url, s.account, s.generation]);
export const EMPTY_PROGRESS: Record<string, Entry> = {};
const valid = (v: ProgressSummary) =>
  !!v &&
  Number.isInteger(v.accepted) &&
  Number.isInteger(v.total) &&
  v.accepted >= 0 &&
  v.total >= v.accepted;
const emptyEntry = (): Entry => ({ updatedAt: 0, retryAfter: 0, force: false, loading: false });

export function createProgressCache(
  fetchProgress: (
    scope: ProgressScope,
    contestId: string,
    force: boolean,
  ) => Promise<ProgressSummary & { updatedAt?: number }>,
  now = Date.now,
) {
  const store = createStore<Record<string, ScopeState>>({});
  const requests = new Map<string, Promise<ProgressSummary | undefined>>();
  const requestKey = (s: ProgressScope, id: string) => JSON.stringify([progressScopeKey(s), id]);
  const update = (scope: ProgressScope, run: (s: ScopeState) => ScopeState) =>
    store.setState((state) => {
      const key = progressScopeKey(scope);
      return { ...state, [key]: run(state[key] ?? { scope, entries: {}, revision: 0 }) };
    });
  const ensure = (scope: ProgressScope, id: string): Promise<ProgressSummary | undefined> => {
    const key = requestKey(scope, id);
    const current = store.get()[progressScopeKey(scope)]?.entries[id];
    const pending = requests.get(key);
    if (pending) return pending;
    const age = now() - (current?.updatedAt ?? 0);
    if (
      current &&
      ((current.value && !current.force && age >= 0 && age < PROGRESS_TTL) ||
        current.retryAfter > now())
    )
      return Promise.resolve(current.value);
    update(scope, (s) => ({
      ...s,
      entries: { ...s.entries, [id]: { ...(current ?? emptyEntry()), loading: true } },
    }));
    const request = Promise.resolve()
      .then(() => fetchProgress(scope, id, current?.force ?? false))
      .then((value) => {
        if (!valid(value)) throw new Error("测验完成状态无效");
        if (requests.get(key) !== request) return undefined;
        update(scope, (s) => ({
          ...s,
          entries: {
            ...s.entries,
            [id]: {
              value: { accepted: value.accepted, total: value.total },
              updatedAt: value.updatedAt ?? now(),
              retryAfter: 0,
              force: false,
              loading: false,
            },
          },
        }));
        return value;
      })
      .catch(() => {
        if (requests.get(key) === request)
          update(scope, (s) => ({
            ...s,
            entries: {
              ...s.entries,
              [id]: { ...s.entries[id], loading: false, retryAfter: now() + RETRY_DELAY },
            },
          }));
        return undefined;
      })
      .finally(() => {
        if (requests.get(key) === request) requests.delete(key);
      });
    requests.set(key, request);
    return request;
  };
  const invalidate = (scope: ProgressScope, ids: string[], force = false) => {
    for (const id of ids) requests.delete(requestKey(scope, id));
    update(scope, (s) => {
      const entries = { ...s.entries };
      for (const id of ids)
        entries[id] = {
          ...(entries[id] ?? emptyEntry()),
          updatedAt: Number.NEGATIVE_INFINITY,
          retryAfter: 0,
          force: force || entries[id]?.force || false,
          loading: false,
        };
      return { ...s, entries, revision: s.revision + 1 };
    });
  };
  return {
    store,
    ensure,
    invalidate,
    seed(scope: ProgressScope, id: string, value: ProgressSummary) {
      if (!valid(value)) return;
      requests.delete(requestKey(scope, id));
      update(scope, (s) => ({
        ...s,
        entries: {
          ...s.entries,
          [id]: { value, updatedAt: now(), retryAfter: 0, force: false, loading: false },
        },
      }));
    },
    invalidateNative(event: {
      siteId: string;
      account: string;
      sessionGeneration: number;
      contestId: string;
    }) {
      for (const { scope } of Object.values(store.get()))
        if (
          scope.siteId === event.siteId &&
          scope.account === event.account &&
          scope.generation === event.sessionGeneration
        )
          invalidate(scope, [event.contestId]);
    },
    reconcile(sites: Site[]) {
      const validScopes = new Set(
        sites.filter((s) => s.enabled).map((s) => progressScopeKey(progressScope(s))),
      );
      const keep: Record<string, ScopeState> = {};
      for (const [key, value] of Object.entries(store.get())) {
        if (validScopes.has(key)) keep[key] = value;
        else
          for (const id of Object.keys(value.entries)) requests.delete(requestKey(value.scope, id));
      }
      store.setState(() => keep);
    },
    clear() {
      requests.clear();
      store.setState(() => ({}));
    },
  };
}
export const ojProgressCache = createProgressCache((scope, contestId, force) =>
  invoke("get_oj_progress", {
    siteId: scope.siteId,
    contestId,
    account: scope.account,
    sessionGeneration: scope.generation,
    force,
  }),
);
