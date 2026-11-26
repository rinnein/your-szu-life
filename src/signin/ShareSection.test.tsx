/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { ShareSection } from "./ShareSection";
import type { Async } from "../async-state";
import type { ShareActions } from "./ShareSection";

const camera = vi.hoisted(() => ({
  state: { status: "ready", data: true } as Async<boolean>,
}));

vi.mock("../qr-scan", () => ({ useCameraAvailability: () => camera.state }));

const actions: ShareActions = {
  scanOnce: vi.fn(async () => undefined),
  startMatching: vi.fn(async () => undefined),
  stop: vi.fn(async () => undefined),
  promptShare: vi.fn(),
  cancelSharePrompt: vi.fn(),
  startSharing: vi.fn(async () => undefined),
};

beforeEach(() => {
  camera.state = { status: "ready", data: true };
});

afterEach(cleanup);

function renderSection() {
  return render(
    <ShareSection rollcallId={1} courseId={2} state={{ stage: "idle" }} actions={actions} />,
  );
}

describe("ShareSection", () => {
  it("offers both scanning actions when a camera is available", () => {
    renderSection();

    expect(screen.getByRole("button", { name: "扫码签到" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "持续共享签到码" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "等待他人共享" })).toBeInTheDocument();
  });

  it("keeps passive matching and explains the missing camera", () => {
    camera.state = { status: "ready", data: false };
    renderSection();

    expect(screen.queryByRole("button", { name: "扫码签到" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "持续共享签到码" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "等待他人共享" })).toBeInTheDocument();
    expect(screen.getByText(/未检测到摄像头/)).toBeInTheDocument();
  });
});
