/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import { WelcomeStep } from "./WelcomeStep";

function renderStep() {
  const onComplete = vi.fn();
  const registerNext = vi.fn();
  render(<WelcomeStep onComplete={onComplete} onSkip={vi.fn()} registerNext={registerNext} />);
  return { onComplete, registerNext };
}

describe("welcome step", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.invoke.mockResolvedValue({ enabled: true, demoMode: true, logCount: 0 });
  });
  afterEach(cleanup);

  it("registers 开始配置 as the wizard footer action", () => {
    const { registerNext } = renderStep();
    const calls = registerNext.mock.calls;
    const action = calls[calls.length - 1]?.[0];
    expect(action?.label).toBe("开始配置");
  });

  it("activates demo mode after five rapid brand taps", async () => {
    renderStep();
    const brand = screen.getByRole("button", {
      name: "应用图标，连续点击 5 次进入演示模式",
    });
    for (let index = 0; index < 5; index += 1) {
      fireEvent.click(brand);
    }
    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith("set_demo_mode", { enabled: true });
    });
  });

  it("does not activate demo mode on a single tap", async () => {
    renderStep();
    const brand = screen.getByRole("button", {
      name: "应用图标，连续点击 5 次进入演示模式",
    });
    fireEvent.click(brand);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
