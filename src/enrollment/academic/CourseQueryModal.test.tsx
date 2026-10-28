/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { typeInto } from "../../ui/testing";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { academicFixture } from "../testing/fixtures";
import { buildTree } from "./tree";
import CourseQueryModal from "./CourseQueryModal";
const push = vi.fn();
vi.mock("../../logical-navigation", () => ({ useLogicalNavigation: () => ({ push }) }));
beforeEach(() => {
  vi.clearAllMocks();
  Element.prototype.getAnimations = () => [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it("opens an ancestor as a searchable paginated table and closes with Escape", async () => {
  const close = vi.fn();
  render(
    <CourseQueryModal
      tree={buildTree(academicFixture)}
      nodeId="group:general"
      viewKey="modal-test"
      onClose={close}
    />,
  );
  await screen.findByRole("grid", { name: "通识教育课程检索" });
  expect(screen.getByText(/共 48 门课程/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "下一页" }));
  expect(screen.getByText("2 / 4")).toBeInTheDocument();
  typeInto(screen.getByLabelText("课程名称或代码"), "GE001");
  await waitFor(() => expect(screen.getByText(/共 1 门课程/)).toBeInTheDocument());
  expect(screen.getByText("人文与艺术素养、科学思维与创新")).toBeInTheDocument();
  expect(screen.getAllByText("记录存在差异").some((el) => el.tagName !== "OPTION")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "查找开课" }));
  expect(push).toHaveBeenCalledWith("/study/enrollment?keyword=GE001");
  const dialog = screen.getByRole("dialog");
  fireEvent(dialog, new CustomEvent("close"));
  await waitFor(() => expect(close).toHaveBeenCalled());
});
