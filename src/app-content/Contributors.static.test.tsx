/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { Contributors } from "./Contributors";

const staticSnapshot = vi.hoisted(() => ({
  repository: { fullName: "owner/repo", url: "https://github.com/owner/repo" },
  contributors: [
    {
      login: "alice",
      avatarUrl: "/assets/avatar-alice.hash.png",
      url: "https://github.com/alice",
    },
  ],
  truncated: false,
}));

vi.mock("./contributors-static", () => ({ staticContributors: staticSnapshot }));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));

vi.mock("../components", () => ({
  Card: ({
    title,
    description,
    children,
  }: {
    title?: string;
    description?: string;
    children: React.ReactNode;
  }) => (
    <section>
      <h2>{title}</h2>
      {description && <p>{description}</p>}
      {children}
    </section>
  ),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("Contributors with a build snapshot", () => {
  it("renders the bundled list without invoking the runtime command", () => {
    const { container } = render(<Contributors />);

    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /owner\/repo/ })).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: /重新加载贡献者/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/缓存于/)).not.toBeInTheDocument();
    expect(screen.getByText(/列表来自公开 GitHub 仓库/)).toBeInTheDocument();
    expect(screen.queryByText(/缓存 24 小时/)).not.toBeInTheDocument();

    const image = container.querySelector(".avatar-image");
    expect(image).toHaveAttribute("src", "/assets/avatar-alice.hash.png");
  });
});
