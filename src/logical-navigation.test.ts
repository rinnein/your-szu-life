import { describe, expect, it } from "vite-plus/test";
import { initialLogicalEntries, isChildDocument } from "./logical-navigation";

describe("logical navigation entry stack", () => {
  it("seeds a direct feature link with the workbench parent", () => {
    expect(initialLogicalEntries("https://tauri.localhost/settings?tab=account#security")).toEqual([
      "/",
      "/settings?tab=account#security",
    ]);
  });

  it("keeps a child Activity as its own logical root", () => {
    const href = "https://tauri.localhost/settings?tab=account&__szu_window=child#security";
    expect(isChildDocument(href)).toBe(true);
    expect(initialLogicalEntries(href)).toEqual(["/settings?tab=account#security"]);
  });

  it("keeps the workbench as the root entry", () => {
    expect(initialLogicalEntries("https://tauri.localhost/")).toEqual(["/"]);
  });
});

it("includes the computing collection for an OJ deep link", () => {
  expect(initialLogicalEntries("https://tauri.localhost/oj?site=szu-67")).toEqual([
    "/",
    "/computing",
    "/oj?site=szu-67",
  ]);
  expect(initialLogicalEntries("https://tauri.localhost/oj?__szu_window=child")).toEqual(["/oj"]);
});

it("restores enrollment parents for nested, legacy and authentication return links", () => {
  for (const path of [
    "/enrollment/tasks",
    "/enrollment?tab=tasks",
    "/enrollment/login?returnTo=%2Fenrollment%2Ftasks",
  ]) {
    expect(initialLogicalEntries(`https://tauri.localhost${path}`)).toEqual([
      "/",
      "/enrollment",
      path,
    ]);
  }
  expect(initialLogicalEntries("https://tauri.localhost/enrollment/login")).toEqual([
    "/",
    "/enrollment/login",
  ]);
});
