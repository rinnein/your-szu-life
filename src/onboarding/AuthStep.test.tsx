/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { toast } from "../ui";
import { typeInto, waitForUpgrade } from "../ui/testing";
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

const pairApi = vi.hoisted(() => ({
  pairLoginHostStart: vi.fn(),
}));

vi.mock("../auth", () => ({
  useAuth: () => auth,
  beginAuthentication: auth.beginAuthentication,
}));

vi.mock("./api", () => api);

vi.mock("../pair-login/api", () => pairApi);

vi.mock("../pair-login/HostPanel", () => ({
  PairHostPanel: ({
    onCancel,
    onAuthorized,
  }: {
    onCancel: () => void;
    onAuthorized: (credentials: { campusCardNumber: string; password: string }) => void;
  }) => (
    <div>
      <button
        type="button"
        onClick={() => onAuthorized({ campusCardNumber: "654321", password: "remote-pass" })}
      >
        交付凭据
      </button>
      <button type="button" onClick={onCancel}>
        取消配对
      </button>
    </div>
  ),
}));

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

function cardField(container: HTMLElement): HTMLElement {
  return container.querySelectorAll("m-text-field")[0] as HTMLElement;
}

function cardInput(container: HTMLElement): HTMLInputElement {
  const input = cardField(container).shadowRoot?.querySelector("input");
  if (!input) throw new Error("校园卡号字段尚未升级");
  return input;
}

/** material 把校验文案放在 shadow root 的 supporting 行里，标红靠 `--error` 类。 */
function cardErrorText(container: HTMLElement): string | null {
  const helper = cardField(container).shadowRoot?.querySelector(".mtrl-text-field__helper--error");
  return helper?.textContent ?? null;
}

/**
 * 只走真实键盘会发出的那一层事件：inner input 的 `input`。
 *
 * `typeInto` 会额外在宿主上派发 change，因此掩盖了「适配器按旧受控值回写」的问题；
 * 用户键入时 change 要到失焦/回车才出现。这里保留最小事件序列，防止回归。
 */
function typeKeyboard(field: HTMLElement, value: string): HTMLInputElement {
  const input = field.shadowRoot!.querySelector("input") as HTMLInputElement;
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
  });
  return input;
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

  it("keeps credentials typed with input events only and submits them", async () => {
    const { container } = render(<Harness />);
    const fields = container.querySelectorAll("m-text-field");
    await waitForUpgrade(fields[0] as HTMLElement);
    await waitForUpgrade(fields[1] as HTMLElement);

    const card = typeKeyboard(fields[0] as HTMLElement, "123456");
    const password = typeKeyboard(fields[1] as HTMLElement, "s3cret");
    // 适配器会在 input 事件的微任务里按受控 value 回写，键入内容不能被清空。
    await act(async () => {
      await Promise.resolve();
    });
    expect(card.value).toBe("123456");
    expect(password.value).toBe("s3cret");

    fireEvent.click(await screen.findByRole("button", { name: "登录并继续" }));
    await vi.waitFor(() => {
      expect(api.beginSetupAuthentication).toHaveBeenCalledWith("123456", "s3cret");
    });
  });

  it("marks an incomplete campus card with a red error instead of submitting", async () => {
    const danger = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    const { container } = render(<Harness />);
    fill(container, "12345", "<PASSWORD>");
    fireEvent.click(await screen.findByRole("button", { name: "登录并继续" }));

    expect(danger).not.toHaveBeenCalled();
    expect(api.beginSetupAuthentication).not.toHaveBeenCalled();
    expect(cardErrorText(container)).toBe("校园卡号必须是 6 位数字");
    expect(cardInput(container).getAttribute("aria-invalid")).toBe("true");

    // 补齐 6 位后错误立即撤销，无需再次提交。
    typeInto(cardField(container), "123456");
    await act(async () => {
      await Promise.resolve();
    });
    expect(cardErrorText(container)).toBeNull();
    expect(cardInput(container).getAttribute("aria-invalid")).toBeNull();
  });

  it("rejects a campus card longer than six digits as soon as it is typed", async () => {
    const { container } = render(<Harness />);
    const field = cardField(container);
    await waitForUpgrade(field);

    typeInto(field, "1234567");
    await act(async () => {
      await Promise.resolve();
    });

    expect(cardErrorText(container)).toBe("校园卡号必须是 6 位数字");
    expect(cardInput(container).getAttribute("maxlength")).toBe("6");
  });

  it("keeps only digits in the campus card field", async () => {
    const { container } = render(<Harness />);
    const field = cardField(container);
    await waitForUpgrade(field);

    typeInto(field, "12ab34");
    await act(async () => {
      await Promise.resolve();
    });

    expect(cardInput(container).value).toBe("1234");
    expect(cardErrorText(container)).toBeNull();
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
    expect(await screen.findByRole("button", { name: "保存并继续" })).toBeInTheDocument();
  });

  it("starts remote pairing and submits the credentials handed over by the peer", async () => {
    pairApi.pairLoginHostStart.mockResolvedValue({
      status: "waiting",
      code: "A1B2C3",
      deepLink: "your-szu-life://page/pair-login?secret=" + "a".repeat(64),
      expiresAtMs: 1,
    });
    render(<Harness />);

    fireEvent.click(await screen.findByRole("button", { name: "远程授权登录" }));
    await vi.waitFor(() => expect(pairApi.pairLoginHostStart).toHaveBeenCalledTimes(1));
    // 配对期间底栏不提供「登录并继续」。
    await vi.waitFor(() =>
      expect(screen.queryByRole("button", { name: "登录并继续" })).not.toBeInTheDocument(),
    );

    fireEvent.click(await screen.findByRole("button", { name: "交付凭据" }));
    await vi.waitFor(() => {
      expect(api.beginSetupAuthentication).toHaveBeenCalledWith("654321", "remote-pass");
    });
  });

  it("returns to the manual form when pairing is cancelled", async () => {
    pairApi.pairLoginHostStart.mockResolvedValue({
      status: "waiting",
      code: "A1B2C3",
      deepLink: "your-szu-life://page/pair-login?secret=" + "a".repeat(64),
      expiresAtMs: 1,
    });
    const { container } = render(<Harness />);

    fireEvent.click(await screen.findByRole("button", { name: "远程授权登录" }));
    fireEvent.click(await screen.findByRole("button", { name: "取消配对" }));

    await vi.waitFor(() => expect(container.querySelectorAll("m-text-field").length).toBe(2));
    expect(screen.getByRole("button", { name: "登录并继续" })).toBeInTheDocument();
  });
});
