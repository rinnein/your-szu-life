/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "./ui";
import type { StepNextAction } from "./step-next-action";
import { ProfileForm } from "./ProfileForm";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("ProfileForm toast error notifications", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("sends toast.danger when student ID is invalid", () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    render(<ProfileForm onSaved={vi.fn()} />);

    const submitBtn = screen.getByRole("button", { name: "保存并进入工作台" });
    fireEvent.click(submitBtn);

    expect(dangerSpy).toHaveBeenCalledWith("学号必须是 10 位数字");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("sends toast.danger when campus card number is invalid", () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    render(<ProfileForm initial={{ studentId: "1234567890" }} onSaved={vi.fn()} />);

    const submitBtn = screen.getByRole("button", { name: "保存并进入工作台" });
    fireEvent.click(submitBtn);

    expect(dangerSpy).toHaveBeenCalledWith("校园卡号必须是 6 位数字");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("sends toast.danger when password is empty for initial setup", () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    render(
      <ProfileForm
        initial={{ studentId: "1234567890", campusCardNumber: "123456" }}
        onSaved={vi.fn()}
      />,
    );

    const submitBtn = screen.getByRole("button", { name: "保存并进入工作台" });
    fireEvent.click(submitBtn);

    expect(dangerSpy).toHaveBeenCalledWith("密码不能为空");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("sends toast.danger when invoke confirm_profile fails", async () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("网络超时"));

    render(
      <ProfileForm
        initial={{ studentId: "1234567890", campusCardNumber: "123456", hasPassword: true }}
        onSaved={vi.fn()}
      />,
    );

    const submitBtn = screen.getByRole("button", { name: "保存并进入工作台" });
    fireEvent.click(submitBtn);

    await vi.waitFor(() => {
      expect(dangerSpy).toHaveBeenCalledWith("保存账户资料失败", {
        description: "Error: 网络超时",
      });
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("registers 保存并进入工作台 as the onboarding footer action", async () => {
    const onSaved = vi.fn();
    const actions: Array<StepNextAction | null> = [];
    vi.mocked(invoke).mockResolvedValue(undefined);

    render(
      <ProfileForm
        onboarding
        initial={{ studentId: "1234567890", campusCardNumber: "123456", hasPassword: true }}
        onSaved={onSaved}
        registerNext={(action) => actions.push(action)}
      />,
    );

    expect(screen.queryByRole("button", { name: "保存并进入工作台" })).not.toBeInTheDocument();
    await waitFor(() => expect(actions[actions.length - 1]?.label).toBe("保存并进入工作台"));

    await actions[actions.length - 1]?.run();
    await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1));
  });
});
