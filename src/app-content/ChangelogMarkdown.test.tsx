/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { ChangelogMarkdown } from "./ChangelogMarkdown";
import { openPublicUrl } from "./external";

vi.mock("./external", () => ({
  openPublicUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const pngBytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0])
  .buffer;

beforeEach(() => {
  Object.defineProperty(URL, "createObjectURL", {
    value: vi.fn(() => "blob:changelog-image"),
    configurable: true,
    writable: true,
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    value: vi.fn(),
    configurable: true,
    writable: true,
  });
  vi.mocked(invoke).mockResolvedValue(pngBytes);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ChangelogMarkdown", () => {
  it("renders headings, emphasis, lists and inline code", () => {
    render(
      <ChangelogMarkdown source={"## 修复\n\n- **重要**：修复登录\n- 兼容 `config` 写法\n"} />,
    );

    expect(screen.getByRole("heading", { level: 2, name: "修复" })).toBeInTheDocument();
    expect(screen.getByText("重要").tagName).toBe("STRONG");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText("config").tagName).toBe("CODE");
  });

  it("opens links in the system browser instead of navigating the webview", () => {
    render(<ChangelogMarkdown source={"[发布页](https://example.com/release)"} />);

    fireEvent.click(screen.getByRole("link", { name: "发布页" }));

    expect(openPublicUrl).toHaveBeenCalledWith("https://example.com/release");
  });

  it("drops executable and relative links", () => {
    const { container } = render(
      <ChangelogMarkdown source={"[坏链接](javascript:alert(1)) 与 [相对](/a)"} />,
    );

    expect(container.querySelector("a")).toBeNull();
  });

  it("lazy-loads images through the rust proxy and keeps alt text", async () => {
    const { container } = render(
      <ChangelogMarkdown source={"![截图](https://example.com/a.png)"} />,
    );

    await waitFor(() => expect(container.querySelector("img")).not.toBeNull());
    const image = container.querySelector("img");
    expect(image).toHaveAttribute("src", "blob:changelog-image");
    expect(image).toHaveAttribute("alt", "截图");
    expect(image).toHaveAttribute("loading", "lazy");
    expect(invoke).toHaveBeenCalledWith("load_remote_resource", {
      url: "https://example.com/a.png",
      kind: "image",
    });
  });
});
