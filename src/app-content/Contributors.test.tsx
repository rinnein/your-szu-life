/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { Contributors, LiveContributors } from "./Contributors";
import type { ContributorsSnapshot } from "./types";

vi.mock("./contributors-static", () => ({ staticContributors: null }));

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

function snapshot(overrides: Partial<ContributorsSnapshot> = {}): ContributorsSnapshot {
  return {
    status: "ready",
    repository: { fullName: "owner/repo", url: "https://github.com/owner/repo" },
    contributors: [
      {
        login: "alice",
        avatarUrl: "https://avatars.example/alice.png",
        url: "https://github.com/alice",
      },
    ],
    truncated: false,
    fromCache: false,
    lastSuccessAt: null,
    error: null,
    message: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(invoke).mockResolvedValue(snapshot());
});

afterEach(() => {
  cleanup();
});

describe("LiveContributors", () => {
  it("loads contributors through the runtime command", async () => {
    render(<LiveContributors />);

    expect(await screen.findByText("alice")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_contributors");
  });

  it("retries after a failed load", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(snapshot());

    render(<LiveContributors />);

    const retry = await screen.findByRole("button", { name: /重新加载贡献者/ });
    fireEvent.click(retry);

    expect(await screen.findByText("alice")).toBeInTheDocument();
    const contributorCalls = vi
      .mocked(invoke)
      .mock.calls.filter(([command]) => command === "get_contributors");
    expect(contributorCalls).toHaveLength(2);
  });
});

describe("Contributors", () => {
  it("falls back to the runtime command when no build snapshot exists", async () => {
    render(<Contributors />);

    expect(await screen.findByText("alice")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_contributors");
  });
});
