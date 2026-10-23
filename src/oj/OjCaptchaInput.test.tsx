/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "@heroui/react";
import OjCaptchaInput from "./OjCaptchaInput";
import type { Site } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const mockSite: Site = {
  id: "szu-oj",
  name: "深大 OJ",
  url: "http://oj.szu.edu.cn/",
  builtin: true,
  reminders: true,
  enabled: true,
  username: "2026000001",
  session: "captchaRequired",
  message: "自动识别未完成，请手填验证码",
};

const initialCaptcha = {
  token: "token-123",
  image:
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") {
      return {
        status: "ready",
        revision: "rev",
        sha256: "sha",
        bytes: 1000,
      };
    }
    if (command === "get_oj_captcha") return initialCaptcha;
    if (command === "auto_recognize_oj_captcha") return "a8b2";
    if (command === "login_oj") return null;
    return undefined;
  });
});

afterEach(() => {
  cleanup();
});

describe("OjCaptchaInput layout and interaction", () => {
  it("renders captcha image, with 换一张 and 自动识别 beside it, and 登录 below input", async () => {
    render(<OjCaptchaInput site={mockSite} />);

    // Captcha image
    const img = await screen.findByAltText("OJ 登录验证码");
    expect(img).toBeInTheDocument();

    // Buttons beside image
    const refreshBtn = screen.getByRole("button", { name: /换一张/ });
    const autoBtn = screen.getByRole("button", { name: /自动识别/ });
    expect(refreshBtn).toBeInTheDocument();
    expect(autoBtn).toBeInTheDocument();

    // Input box
    const input = screen.getByLabelText("验证码");
    expect(input).toBeInTheDocument();

    // Login button below input
    const loginBtn = screen.getByRole("button", { name: /登录/ });
    expect(loginBtn).toBeInTheDocument();
    expect(loginBtn).toBeDisabled(); // Disabled when input is empty
  });

  it("clicks 换一张 to refresh captcha and clears answer", async () => {
    render(<OjCaptchaInput site={mockSite} />);
    const input = await screen.findByLabelText("验证码");
    fireEvent.change(input, { target: { value: "1234" } });
    expect(input).toHaveValue("1234");

    const newCaptcha = { token: "token-456", image: "data:image/png;base64,new" };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_captcha") return newCaptcha;
      return undefined;
    });

    const refreshBtn = screen.getByRole("button", { name: /换一张/ });
    fireEvent.click(refreshBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_oj_captcha", { siteId: "szu-oj", refresh: true });
      expect(input).toHaveValue("");
    });
  });

  it("clicks 自动识别 to recognize captcha and fill answer", async () => {
    const successSpy = vi.spyOn(toast, "success");
    render(<OjCaptchaInput site={mockSite} />);
    const input = await screen.findByLabelText("验证码");

    const autoBtn = screen.getByRole("button", { name: /自动识别/ });
    fireEvent.click(autoBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("auto_recognize_oj_captcha", { siteId: "szu-oj" });
      expect(input).toHaveValue("a8b2");
      expect(successSpy).toHaveBeenCalledWith("自动识别完成：a8b2");
    });

    // Login button is enabled now
    const loginBtn = screen.getByRole("button", { name: /登录/ });
    expect(loginBtn).toBeEnabled();
  });

  it("submits answer and triggers onAuthenticated on login success", async () => {
    const onAuth = vi.fn();
    render(<OjCaptchaInput site={mockSite} onAuthenticated={onAuth} />);
    const input = await screen.findByLabelText("验证码");
    fireEvent.change(input, { target: { value: "a8b2" } });

    const loginBtn = screen.getByRole("button", { name: /登录/ });
    fireEvent.click(loginBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("login_oj", {
        siteId: "szu-oj",
        token: "token-123",
        value: "a8b2",
      });
      expect(onAuth).toHaveBeenCalledOnce();
    });
  });

  it("triggers onModelRequired when OCR model is missing during 自动识别", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_ocr_model_status") return { status: "missing", bytes: 1000 };
      if (command === "get_oj_captcha") return initialCaptcha;
      return undefined;
    });

    const onModelRequired = vi.fn();
    const warnSpy = vi.spyOn(toast, "warning");

    render(<OjCaptchaInput site={mockSite} onModelRequired={onModelRequired} />);
    await screen.findByAltText("OJ 登录验证码");

    const autoBtn = screen.getByRole("button", { name: /自动识别/ });
    fireEvent.click(autoBtn);

    await waitFor(() => {
      expect(onModelRequired).toHaveBeenCalledOnce();
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("安装本地 OCR 模型"));
      expect(invoke).not.toHaveBeenCalledWith("auto_recognize_oj_captcha", expect.anything());
    });
  });
});
