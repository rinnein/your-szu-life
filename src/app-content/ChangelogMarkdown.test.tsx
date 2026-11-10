/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { ChangelogMarkdown } from "./ChangelogMarkdown";
import { openPublicUrl } from "./external";

vi.mock("./external", () => ({
  openPublicUrl: vi.fn().mockResolvedValue(undefined),
}));

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

  it("lazy-loads images and keeps alt text", () => {
    const { container } = render(
      <ChangelogMarkdown source={"![截图](https://example.com/a.png)"} />,
    );

    const image = container.querySelector("img");
    expect(image).toHaveAttribute("src", "https://example.com/a.png");
    expect(image).toHaveAttribute("alt", "截图");
    expect(image).toHaveAttribute("loading", "lazy");
  });
});
