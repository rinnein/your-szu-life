/** @vitest-environment jsdom */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vite-plus/test";
import ComputingCollectionPage from "./ComputingCollectionPage";
const nav = vi.hoisted(() => ({ push: vi.fn(), back: vi.fn() }));
vi.mock("./logical-navigation", () => ({ useLogicalNavigation: () => nav }));
afterEach(cleanup);
it("opens OJ as another logical page and returns using the shared bridge", () => {
  render(<ComputingCollectionPage />);
  fireEvent.click(screen.getByRole("button", { name: /OJ/ }));
  expect(nav.push).toHaveBeenCalledWith("/oj");
  fireEvent.click(screen.getByRole("button", { name: /返回/ }));
  expect(nav.back).toHaveBeenCalled();
});
