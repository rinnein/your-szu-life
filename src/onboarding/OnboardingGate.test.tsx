/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { OnboardingGate } from "./OnboardingGate";

const auth = vi.hoisted(() => ({
  state: { Authenticated: { studentId: "1234567890", name: "测试" } } as Record<string, unknown>,
}));

const api = vi.hoisted(() => ({
  getOnboardingState: vi.fn(),
  completeOnboardingStep: vi.fn(),
  skipOnboardingStep: vi.fn(),
}));

vi.mock("../auth", () => ({
  useAuth: () => auth,
}));

vi.mock("./api", () => api);

vi.mock("./registry", () => ({
  STEP_META: {
    auth: { label: "账户与个人信息", component: () => <div>auth-step</div> },
    appearance: { label: "主题设置", component: () => <div>appearance-step</div> },
    dormitory: { label: "宿舍信息", component: () => <div>dormitory-step</div> },
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
  LoadingIndicator: () => <span data-testid="loading" />,
  Progress: () => <div data-testid="progress" />,
}));

function step(id: string, status: string, skippable = false) {
  return { id, status, skippable, revision: 1 };
}

const completeState = {
  steps: [
    step("auth", "configured"),
    step("dormitory", "skipped", true),
    step("appearance", "configured"),
    step("local_model", "skipped", true),
    step("ai", "skipped", true),
    step("mcp", "configured"),
  ],
  needsSetup: false,
  currentStep: null,
};

describe("OnboardingGate", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    auth.state = { Authenticated: { studentId: "1234567890", name: "测试" } };
  });

  afterEach(() => cleanup());

  it("renders the app when onboarding is complete and auth is valid", async () => {
    api.getOnboardingState.mockResolvedValue(completeState);
    render(
      <OnboardingGate>
        <div>app-content</div>
      </OnboardingGate>,
    );
    expect(await screen.findByText("app-content")).toBeInTheDocument();
  });

  it("renders the next pending step when onboarding is incomplete", async () => {
    api.getOnboardingState.mockResolvedValue({
      ...completeState,
      needsSetup: true,
      currentStep: "appearance",
      steps: completeState.steps.map((item) =>
        item.id === "appearance" ? { ...item, status: "pending" } : item,
      ),
    });
    render(
      <OnboardingGate>
        <div>app-content</div>
      </OnboardingGate>,
    );
    expect(await screen.findByText("appearance-step")).toBeInTheDocument();
    expect(screen.queryByText("app-content")).not.toBeInTheDocument();
  });

  it("opens the auth step when onboarding is complete but login was lost", async () => {
    auth.state = { Unauthenticated: null };
    api.getOnboardingState.mockResolvedValue(completeState);
    render(
      <OnboardingGate>
        <div>app-content</div>
      </OnboardingGate>,
    );
    expect(await screen.findByText("auth-step")).toBeInTheDocument();
    expect(screen.queryByText("app-content")).not.toBeInTheDocument();
  });

  it("allows dismissing a re-auth-only wizard", async () => {
    auth.state = { Unauthenticated: null };
    api.getOnboardingState.mockResolvedValue(completeState);
    render(
      <OnboardingGate>
        <div>app-content</div>
      </OnboardingGate>,
    );
    await screen.findByText("auth-step");
    fireEvent.click(screen.getByRole("button", { name: "稍后处理" }));
    await waitFor(() => expect(screen.getByText("app-content")).toBeInTheDocument());
  });
});
