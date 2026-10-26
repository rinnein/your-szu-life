/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { toast } from "./ui";
import {
  initialLogicalEntries,
  isChildDocument,
  LogicalNavigationProvider,
  useLogicalNavigation,
} from "./logical-navigation";
import { useRouter } from "@tanstack/react-router";

vi.mock("@tanstack/react-router", () => ({
  useRouter: vi.fn(),
}));

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

function TestNavConsumer() {
  const nav = useLogicalNavigation();
  return (
    <button type="button" onClick={() => void nav.push("/fail")}>
      Navigate
    </button>
  );
}

describe("LogicalNavigationProvider toast error", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("triggers toast.danger when push fails", async () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    vi.mocked(useRouter).mockReturnValue({
      history: { canGoBack: () => false, back: vi.fn() },
      navigate: vi.fn().mockRejectedValue(new Error("navigation failed")),
    } as unknown as ReturnType<typeof useRouter>);

    render(
      <LogicalNavigationProvider>
        <TestNavConsumer />
      </LogicalNavigationProvider>,
    );

    const button = screen.getByRole("button", { name: "Navigate" });
    button.click();

    await vi.waitFor(() => {
      expect(dangerSpy).toHaveBeenCalledWith("页面未打开", {
        description: "无法打开独立页面，请重试；当前页面已保留。",
      });
    });
  });
});
