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

it("includes the study hub for an OJ deep link", () => {
  expect(initialLogicalEntries("https://tauri.localhost/study/oj?site=szu-67")).toEqual([
    "/",
    "/study",
    "/study/oj?site=szu-67",
  ]);
  expect(initialLogicalEntries("https://tauri.localhost/study/oj?__szu_window=child")).toEqual([
    "/study/oj",
  ]);
});

it("derives the parent chain from the nested route path", () => {
  expect(initialLogicalEntries("https://tauri.localhost/study/uooc?tab=tasks")).toEqual([
    "/",
    "/study",
    "/study/uooc?tab=tasks",
  ]);
  expect(initialLogicalEntries("https://tauri.localhost/life")).toEqual(["/", "/life"]);
  expect(initialLogicalEntries("https://tauri.localhost/life/campus-payments/electricity")).toEqual(
    ["/", "/life", "/life/campus-payments", "/life/campus-payments/electricity"],
  );
});

it("restores enrollment parents for nested, legacy and authentication return links", () => {
  for (const path of [
    "/study/enrollment/tasks",
    "/study/enrollment?tab=tasks",
    "/study/enrollment/login?returnTo=%2Fstudy%2Fenrollment%2Ftasks",
  ]) {
    expect(initialLogicalEntries(`https://tauri.localhost${path}`)).toEqual([
      "/",
      "/study",
      "/study/enrollment",
      path,
    ]);
  }
  expect(initialLogicalEntries("https://tauri.localhost/study/enrollment/login")).toEqual([
    "/",
    "/study",
    "/study/enrollment/login",
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
