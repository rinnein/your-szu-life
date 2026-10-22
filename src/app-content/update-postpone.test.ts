/** @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import {
  clearPostponedUpdateReminder,
  isUpdatePostponed,
  postponeUpdateReminder,
  UPDATE_POSTPONED_KEY,
} from "./update-postpone";

describe("update-postpone", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("returns false when nothing is in localStorage", () => {
    expect(isUpdatePostponed()).toBe(false);
  });

  it("returns true when postponed timestamp is in the future", () => {
    const future = Date.now() + 1000 * 60;
    localStorage.setItem(UPDATE_POSTPONED_KEY, future.toString());
    expect(isUpdatePostponed()).toBe(true);
  });

  it("returns false when postponed timestamp is in the past", () => {
    const past = Date.now() - 1000;
    localStorage.setItem(UPDATE_POSTPONED_KEY, past.toString());
    expect(isUpdatePostponed()).toBe(false);
  });

  it("postponeUpdateReminder sets timestamp 24h into the future by default", () => {
    const now = 1_000_000_000_000;
    vi.spyOn(Date, "now").mockReturnValue(now);

    postponeUpdateReminder();
    expect(localStorage.getItem(UPDATE_POSTPONED_KEY)).toBe((now + 24 * 60 * 60 * 1000).toString());
    expect(isUpdatePostponed()).toBe(true);
  });

  it("clearPostponedUpdateReminder removes key", () => {
    localStorage.setItem(UPDATE_POSTPONED_KEY, (Date.now() + 100000).toString());
    expect(isUpdatePostponed()).toBe(true);

    clearPostponedUpdateReminder();
    expect(localStorage.getItem(UPDATE_POSTPONED_KEY)).toBeNull();
    expect(isUpdatePostponed()).toBe(false);
  });
});
