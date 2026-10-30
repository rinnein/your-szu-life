/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { toast } from "../ui";
import { typeInto } from "../ui/testing";
import type { StepNextAction } from "../step-next-action";
import { AuthStep } from "./AuthStep";

const auth = vi.hoisted(() => ({
  state: { Unauthenticated: null } as Record<string, unknown>,
  profile: undefined as Record<string, unknown> | undefined,
  userData: undefined as Record<string, unknown> | undefined,
  beginAuthentication: vi.fn(),
}));

const api = vi.hoisted(() => ({
  beginSetupAuthentication: vi.fn(),
  cancelAuthentication: vi.fn(),
  consumeAuthFailureNotice: vi.fn(),
}));

vi.mock("../auth", () => ({
  useAuth: () => auth,
  beginAuthentication: auth.beginAuthentication,
}));

vi.mock("./api", () => api);

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function Harness({
  onComplete = vi.fn(),
  reauth,
}: {
  onComplete?: () => void | Promise<void>;
  reauth?: boolean;
}) {
  const [action, setAction] = useState<StepNextAction | null>(null);
  return (
    <>
      <AuthStep onComplete={onComplete} onSkip={vi.fn()} reauth={reauth} registerNext={setAction} />
      {action && (
        <button type="button" disabled={action.disabled} onClick={() => void action.run()}>
          {action.label}
        </button>
      )}
    </>
  );
}

function fill(container: HTMLElement, campusCard: string, password: string) {
  const fields = container.querySelectorAll("m-text-field");
  typeInto(fields[0] as HTMLElement, campusCard);
  typeInto(fields[1] as HTMLElement, password);
}

describe("AuthStep", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    auth.state = { Unauthenticated: null };
    auth.profile = undefined;
    auth.userData = undefined;
    auth.beginAuthentication.mockResolvedValue(undefined);
    api.beginSetupAuthentication.mockResolvedValue(undefined);
    api.cancelAuthentication.mockResolvedValue(undefined);
    api.consumeAuthFailureNotice.mockResolvedValue(null);
  });

  afterEach(() => cleanup());

  it("submits the campus card and password to begin_setup_authentication", async () => {
    const { container } = render(<Harness />);
    fill(container, "123456", "<PASSWORD>");
    fireEvent.click(await screen.findByRole("button", { name: "登录并继续" }));

    await vi.waitFor(() => {
      expect(api.beginSetupAuthentication).toHaveBeenCalledWith("123456", "<PASSWORD>");
    });
  });

  it("rejects a campus card that is not six digits", async () => {
    const danger = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    const { container } = render(<Harness />);
    fill(container, "12345", "<PASSWORD>");
    fireEvent.click(await screen.findByRole("button", { name: "登录并继续" }));

    expect(danger).toHaveBeenCalledWith("校园卡号必须是 6 位数字");
    expect(api.beginSetupAuthentication).not.toHaveBeenCalled();
  });

  it("shows the consumed failure notice as a snackbar", async () => {
    const danger = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    api.consumeAuthFailureNotice.mockResolvedValue("统一认证未完成，请重试。");
    render(<Harness />);
    await vi.waitFor(() => {
      expect(danger).toHaveBeenCalledWith("统一认证未完成", {
        description: "统一认证未完成，请重试。",
      });
    });
  });

  it("silently starts reauthentication when a saved password exists", async () => {
    auth.state = { SessionExpired: null };
    auth.profile = {
      studentId: "1234567890",
      campusCardNumber: "123456",
      hasPassword: true,
      name: "测试",
    };
    render(<Harness reauth />);

    expect(await screen.findByText("正在恢复统一认证")).toBeInTheDocument();
    await vi.waitFor(() => expect(auth.beginAuthentication).toHaveBeenCalledTimes(1));
  });

  it("asks for credentials only when no saved password exists", async () => {
    auth.state = { SessionExpired: null };
    auth.profile = {
      studentId: "1234567890",
      campusCardNumber: "123456",
      hasPassword: false,
      name: "测试",
    };
    render(<Harness reauth />);

    expect(await screen.findByRole("button", { name: "登录并继续" })).toBeInTheDocument();
    expect(auth.beginAuthentication).not.toHaveBeenCalled();
  });

  it("does not delay the form when there is no local profile at all", async () => {
    auth.state = { Unauthenticated: null };
    auth.profile = undefined;
    render(<Harness reauth />);

    expect(await screen.findByRole("button", { name: "登录并继续" })).toBeInTheDocument();
    expect(auth.beginAuthentication).not.toHaveBeenCalled();
  });

  it("reuses ProfileForm when the account is awaiting a campus card", async () => {
    auth.state = { AwaitingProfile: { studentId: "1234567890", name: "测试" } };
    render(<Harness />);

    expect(await screen.findByText("补充校园卡号")).toBeInTheDocument();
    expect(screen.queryByText("账户与个人信息")).not.toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "保存并进入工作台" })).toBeInTheDocument();
  });
});
