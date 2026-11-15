/**
 * 「稍后再说」类提醒的推迟记录（localStorage）。
 *
 * 全局的更新提示与 UOOC 的模型下载提示共用同一套语义：用户关掉提醒后，
 * 同一提醒在推迟窗口内不再出现。key 隔离各自的窗口；就绪后调用
 * `clearPostponed` 撤销记录，让下次触发时重新提醒。
 */
export const ONE_DAY_MS = 24 * 60 * 60 * 1000;

/**
 * 存储键与 key 原样一致：调用方自己传完整键（全局更新提示沿用历史的
 * `update_reminder_postponed_until`），老用户的推迟窗口不被换键重置。
 */
function storageKey(key: string) {
  return key;
}

export function isPostponed(key: string, now: number = Date.now()): boolean {
  try {
    const raw = localStorage.getItem(storageKey(key));
    if (!raw) return false;
    const until = Number(raw);
    return Number.isFinite(until) && now < until;
  } catch {
    return false;
  }
}

export function postpone(key: string, durationMs: number = ONE_DAY_MS, now: number = Date.now()): void {
  try {
    localStorage.setItem(storageKey(key), (now + durationMs).toString());
  } catch {
    // localStorage 不可用（配额/隐私模式）时放弃记录，只是每次都会提醒。
  }
}

export function clearPostponed(key: string): void {
  try {
    localStorage.removeItem(storageKey(key));
  } catch {
    // 同上，忽略。
  }
}
