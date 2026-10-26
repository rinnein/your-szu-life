/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { typeInto } from "./ui/testing";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "./ui";
import SettingsTransfer from "./SettingsTransfer";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("SettingsTransfer toast notifications", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("sends toast.danger when export fails", async () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("密钥派生失败"));

    render(<SettingsTransfer />);

    const passwordInput = screen.getAllByPlaceholderText("至少 8 个字符")[0];
    typeInto(passwordInput, "secret123456");

    const exportBtn = screen.getByRole("button", { name: "生成加密字符串" });
    fireEvent.click(exportBtn);

    await vi.waitFor(() => {
      expect(dangerSpy).toHaveBeenCalledWith("密钥派生失败");
    });
  });

  it("sends toast.danger when import inspection fails", async () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("密码错误或内容已损坏"));

    render(<SettingsTransfer />);

    const textarea = screen.getAllByPlaceholderText("粘贴另一台设备生成的加密字符串")[0];
    typeInto(textarea, "invalid-cipher-text");

    const passwordInput = screen.getAllByPlaceholderText("输入导出时设置的密码")[0];
    typeInto(passwordInput, "secret123456");

    const previewBtn = screen.getByRole("button", { name: "检查设置包" });
    fireEvent.click(previewBtn);

    await vi.waitFor(() => {
      expect(dangerSpy).toHaveBeenCalledWith("密码错误或内容已损坏");
    });
  });
});
