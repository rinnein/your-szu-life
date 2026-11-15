/** @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { clearPostponed, isPostponed, ONE_DAY_MS, postpone } from "./reminder-postpone";

const KEY = "test_reminder_key";

describe("reminder-postpone", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("returns false when nothing was recorded", () => {
    expect(isPostponed(KEY)).toBe(false);
  });

  it("returns true within the window and false afterwards", () => {
    postpone(KEY, ONE_DAY_MS, 1_000_000_000_000);
    expect(isPostponed(KEY, 1_000_000_000_000 + 60_000)).toBe(true);
    expect(isPostponed(KEY, 1_000_000_000_000 + ONE_DAY_MS + 1)).toBe(false);
  });

  it("isolates keys from each other", () => {
    postpone(KEY, ONE_DAY_MS, 1_000_000_000_000);
    expect(isPostponed("other_key")).toBe(false);
    expect(localStorage.getItem(KEY)).toBe((1_000_000_000_000 + ONE_DAY_MS).toString());
  });

  it("defaults to one day", () => {
    postpone(KEY);
    expect(Number(localStorage.getItem(KEY))).toBeGreaterThan(Date.now() + ONE_DAY_MS - 5000);
  });

  it("clearPostponed removes the record", () => {
    postpone(KEY, ONE_DAY_MS);
    expect(isPostponed(KEY)).toBe(true);
    clearPostponed(KEY);
    expect(isPostponed(KEY)).toBe(false);
    expect(localStorage.getItem(KEY)).toBeNull();
  });
});
