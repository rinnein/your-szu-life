import { describe, expect, it } from "vite-plus/test";
import { createAccountCache } from "./account-cache";

describe("createAccountCache", () => {
  it("only returns the value written for the same account", () => {
    const cache = createAccountCache<string[]>();
    expect(cache.read("student-a")).toBeUndefined();

    cache.write("student-a", ["a"]);
    expect(cache.read("student-a")).toEqual(["a"]);
    // 只保留最近一个账号：另一个账号读不到它的数据。
    expect(cache.read("student-b")).toBeUndefined();
    expect(cache.read(undefined)).toBeUndefined();
  });

  it("ignores writes without an account", () => {
    const cache = createAccountCache<number>();
    cache.write(undefined, 1);
    expect(cache.read(undefined)).toBeUndefined();

    cache.write("student-a", 2);
    expect(cache.read("student-a")).toBe(2);
  });
});
