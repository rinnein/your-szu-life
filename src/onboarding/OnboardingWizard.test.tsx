/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { OnboardingWizard } from "./OnboardingWizard";
import type { OnboardingState } from "./api";

const auth = vi.hoisted(() => ({
  state: { Authenticated: { studentId: "1234567890", name: "测试" } } as Record<string, unknown>,
}));

const api = vi.hoisted(() => ({
  completeOnboardingStep: vi.fn(),
  skipOnboardingStep: vi.fn(),
}));

vi.mock("../auth", () => ({ useAuth: () => auth }));
vi.mock("./api", () => api);

vi.mock("./registry", () => ({
  STEP_META: {
    dormitory: {
      label: "宿舍信息",
      component: ({ onComplete, onSkip }: any) => (
        <div>
          <div>dormitory-step</div>
          <button onClick={() => void onComplete()}>complete-dorm</button>
          <button onClick={() => void onSkip()}>skip-dorm</button>
        </div>
      ),
    },
    appearance: {
      label: "主题设置",
      component: () => <div>appearance-step</div>,
    },
  },
}));

vi.mock("../shell/PageSurface", () => ({
  PageSurface: ({ children }: any) => <div>{children}</div>,
}));

vi.mock("../components", () => ({
  BrandMark: () => <div data-testid="brand-mark" />,
}));

vi.mock("material/react", () => ({
  Button: ({ children, onClick, disabled }: any) => (
    <button onClick={onClick} disabled={disabled}>
      {children}
    </button>
  ),
  Card: ({ children }: any) => <div>{children}</div>,
  Progress: () => <div data-testid="progress" />,
}));

const state: OnboardingState = {
  steps: [
    { id: "auth", status: "configured", skippable: false, revision: 1 },
    { id: "dormitory", status: "pending", skippable: true, revision: 1 },
    { id: "appearance", status: "pending", skippable: false, revision: 1 },
    { id: "local_model", status: "skipped", skippable: true, revision: 1 },
    { id: "ai", status: "skipped", skippable: true, revision: 1 },
    { id: "mcp", status: "configured", skippable: false, revision: 1 },
  ],
  needsSetup: true,
  currentStep: "dormitory",
};

describe("OnboardingWizard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.completeOnboardingStep.mockResolvedValue(state);
    api.skipOnboardingStep.mockResolvedValue(state);
  });

  afterEach(() => cleanup());

  it("shows the first pending step and only offers skip for skippable steps", () => {
    render(
      <OnboardingWizard
        state={state}
        canDismiss={false}
        onDismiss={vi.fn()}
        onFinished={vi.fn()}
      />,
    );
    expect(screen.getByText("dormitory-step")).toBeInTheDocument();
    expect(screen.getByText(/第 1 \/ 2 步/, { exact: false })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "跳过" })).toBeInTheDocument();
  });

  it("skips a skippable step and advances", async () => {
    render(
      <OnboardingWizard
        state={state}
        canDismiss={false}
        onDismiss={vi.fn()}
        onFinished={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "跳过" }));
    await screen.findByText("appearance-step");
    expect(api.skipOnboardingStep).toHaveBeenCalledWith("dormitory");
    expect(screen.queryByRole("button", { name: "跳过" })).not.toBeInTheDocument();
  });

  it("marks a step complete and advances", async () => {
    render(
      <OnboardingWizard
        state={state}
        canDismiss={false}
        onDismiss={vi.fn()}
        onFinished={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "complete-dorm" }));
    await screen.findByText("appearance-step");
    expect(api.completeOnboardingStep).toHaveBeenCalledWith("dormitory");
  });
});
