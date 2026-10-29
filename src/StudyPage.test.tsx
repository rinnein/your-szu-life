/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import StudyPage from "./StudyPage";

const nav = vi.hoisted(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }));
vi.mock("./logical-navigation", () => ({ useLogicalNavigation: () => nav }));
vi.mock("./auth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./auth")>();
  return {
    ...actual,
    useAuth: () => ({ state: { Authenticated: { studentId: "123456", name: "测试同学" } } }),
  };
});

describe("StudyPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(cleanup);

  it("直接呈现 OJ 功能卡片且无计软合集卡片，点击导航到 /study/oj", () => {
    render(<StudyPage />);
    expect(screen.queryByText("计软合集")).toBeNull();
    const ojButton = screen.getByRole("button", { name: /OJ.*班级、测验与编程练习/ });
    expect(ojButton).toBeInTheDocument();
    fireEvent.click(ojButton);
    expect(nav.push).toHaveBeenCalledWith("/study/oj");
  });
});
