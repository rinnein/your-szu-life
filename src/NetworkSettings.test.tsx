/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { NetworkSettings, type ProxySettingsView } from "./NetworkSettings";
import { toast } from "./ui";
import { hostOf, typeInto } from "./ui/testing";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./CampusAccessSettings", () => ({ CampusAccessSettings: () => null }));

const initial: ProxySettingsView = {
  mode: "auto",
  protocol: "http",
  host: "127.0.0.1",
  port: 7890,
  username: "",
  hasPassword: false,
  campusThroughProxy: false,
};

type ProxyInput = Omit<ProxySettingsView, "hasPassword"> & { password: string | null };

function mockBackend(overrides: Partial<ProxySettingsView> = {}) {
  const current = { ...initial, ...overrides };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_proxy_settings") return current;
    if (command === "save_proxy_settings") {
      const input = (args as { settings: ProxyInput }).settings;
      return {
        mode: input.mode,
        protocol: input.protocol,
        host: input.host,
        port: input.port,
        username: input.username,
        hasPassword: current.hasPassword || input.password !== null,
        campusThroughProxy: input.campusThroughProxy,
      } satisfies ProxySettingsView;
    }
    if (command === "test_proxy_settings") {
      return { target: "http://example.com", status: 200, elapsedMs: 12 };
    }
    return undefined;
  });
}

beforeEach(() => {
  vi.mocked(listen).mockResolvedValue(() => undefined);
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** 读取完成前控件都禁用，交互前先等它可用。 */
async function readyMode(name: string) {
  const radio = await screen.findByRole("radio", { name });
  await waitFor(() => expect(radio).toBeEnabled());
}

describe("NetworkSettings", () => {
  it("keeps the manual server block inside the proxy mode card", async () => {
    mockBackend();
    render(<NetworkSettings />);

    expect(await screen.findByText("代理模式")).toBeInTheDocument();
    expect(screen.queryByText("代理服务器")).not.toBeInTheDocument();
    await readyMode("手动");

    fireEvent.click(screen.getByRole("radio", { name: "手动" }));

    const serverHeadline = await screen.findByText("代理服务器");
    const group = serverHeadline.closest(".card-group");
    expect(group).not.toBeNull();
    const modeCard = screen.getByText("代理模式").closest("m-card");
    expect(modeCard).not.toBeNull();
    expect(group?.closest("m-card")).toBe(modeCard);

    expect(
      within(modeCard as HTMLElement).getByRole("button", { name: /保存/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "测试连接" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /恢复默认/ })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "自动" }));
    await waitFor(() => expect(screen.queryByText("代理服务器")).not.toBeInTheDocument());
  });

  it("saves only the proxy mode card and keeps the persisted bypass value", async () => {
    mockBackend();
    render(<NetworkSettings />);

    await screen.findByText("代理模式");
    await readyMode("手动");
    fireEvent.click(screen.getByRole("radio", { name: "手动" }));
    typeInto(await screen.findByLabelText("服务器地址"), "proxy.example.com");

    fireEvent.click(screen.getByRole("button", { name: /保存/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_proxy_settings", {
        settings: expect.objectContaining({
          mode: "manual",
          host: "proxy.example.com",
          campusThroughProxy: false,
        }),
      });
    });
  });

  it("applies the campus bypass switch immediately", async () => {
    mockBackend();
    render(<NetworkSettings />);

    const toggle = await screen.findByLabelText("校内网也走代理");
    await waitFor(() => expect(toggle).toBeEnabled());
    toggle.dispatchEvent(new CustomEvent("change", { detail: { checked: true } }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_proxy_settings", {
        settings: expect.objectContaining({ campusThroughProxy: true }),
      });
    });
  });

  it("rolls the bypass switch back when the immediate save fails", async () => {
    const danger = vi.spyOn(toast, "danger").mockImplementation(() => "toast-id");
    mockBackend();
    const backend = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "save_proxy_settings") throw "凭据文档不可写";
      return backend(command, args);
    });

    render(<NetworkSettings />);

    const toggle = await screen.findByLabelText("校内网也走代理");
    await waitFor(() => expect(toggle).toBeEnabled());
    toggle.dispatchEvent(new CustomEvent("change", { detail: { checked: true } }));

    await waitFor(() => expect(danger).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByLabelText("校内网也走代理")).not.toBeChecked());
  });

  it("tests the current proxy configuration", async () => {
    mockBackend();
    render(<NetworkSettings />);

    const testButton = await screen.findByRole("button", { name: "测试连接" });
    await waitFor(() => expect(testButton).toBeEnabled());
    fireEvent.click(testButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "test_proxy_settings",
        expect.objectContaining({ url: null }),
      );
    });
  });

  it("resets mode fields without touching the bypass switch", async () => {
    mockBackend({ campusThroughProxy: true, mode: "manual", host: "proxy.example.com" });
    render(<NetworkSettings />);

    await screen.findByText("代理模式");
    await waitFor(() => expect(screen.getByRole("radio", { name: "手动" })).toBeChecked());
    const bypass = screen.getByLabelText("校内网也走代理") as HTMLElement & {
      checked: boolean;
    };
    expect(bypass.checked).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: /恢复默认/ }));

    await waitFor(() => expect(screen.getByRole("radio", { name: "自动" })).toBeChecked());
    expect(bypass.checked).toBe(true);
  });

  it("rejects a port outside 1 - 65535 before saving or testing", async () => {
    mockBackend();
    render(<NetworkSettings />);

    await screen.findByText("代理模式");
    await readyMode("手动");
    fireEvent.click(screen.getByRole("radio", { name: "手动" }));
    const port = await screen.findByLabelText("端口");
    typeInto(port, "0");

    fireEvent.click(screen.getByRole("button", { name: /保存/ }));

    const field = hostOf(port) as HTMLElement;
    await waitFor(() => expect(field).toHaveAttribute("error"));
    expect(field.getAttribute("supporting-text")).toBe("端口需为 1 - 65535 之间的整数。");
    expect(invoke).not.toHaveBeenCalledWith("save_proxy_settings", expect.anything());

    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));
    expect(invoke).not.toHaveBeenCalledWith("test_proxy_settings", expect.anything());
  });

  it("keeps save enabled while a connection test is in flight", async () => {
    mockBackend();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const backend = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "test_proxy_settings") {
        await gate;
        return { target: "http://example.com", status: 200, elapsedMs: 12 };
      }
      return backend(command, args);
    });

    render(<NetworkSettings />);
    await screen.findByText("代理模式");
    const testButton = await screen.findByRole("button", { name: "测试连接" });
    await waitFor(() => expect(testButton).toBeEnabled());

    fireEvent.click(testButton);

    await waitFor(() => expect(screen.getByRole("button", { name: "测试连接" })).toBeDisabled());
    expect(screen.getByRole("button", { name: /保存/ })).toBeEnabled();

    release();
    await waitFor(() => expect(screen.getByRole("button", { name: "测试连接" })).toBeEnabled());
  });
});
