import { ONE_DAY_MS, clearPostponed, isPostponed, postpone } from "../reminder-postpone";

/** 更新提示的推迟记录 key；沿用历史存储键，已推迟的用户不会被换键重置。 */
export const UPDATE_POSTPONED_KEY = "update_reminder_postponed_until";
export { ONE_DAY_MS };

export function isUpdatePostponed(): boolean {
  return isPostponed(UPDATE_POSTPONED_KEY);
}

export function postponeUpdateReminder(durationMs: number = ONE_DAY_MS): void {
  postpone(UPDATE_POSTPONED_KEY, durationMs);
}

export function clearPostponedUpdateReminder(): void {
  clearPostponed(UPDATE_POSTPONED_KEY);
}
