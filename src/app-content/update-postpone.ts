export const UPDATE_POSTPONED_KEY = "update_reminder_postponed_until";
export const ONE_DAY_MS = 24 * 60 * 60 * 1000;

export function isUpdatePostponed(): boolean {
  try {
    const raw = localStorage.getItem(UPDATE_POSTPONED_KEY);
    if (!raw) return false;
    const until = Number(raw);
    if (!Number.isFinite(until)) return false;
    return Date.now() < until;
  } catch {
    return false;
  }
}

export function postponeUpdateReminder(durationMs: number = ONE_DAY_MS): void {
  try {
    localStorage.setItem(UPDATE_POSTPONED_KEY, (Date.now() + durationMs).toString());
  } catch {
    // Ignore storage quota or disabled localStorage exceptions
  }
}

export function clearPostponedUpdateReminder(): void {
  try {
    localStorage.removeItem(UPDATE_POSTPONED_KEY);
  } catch {
    // Ignore
  }
}
