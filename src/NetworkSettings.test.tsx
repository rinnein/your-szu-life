/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { NetworkSettings, type ProxySettingsView } from "./NetworkSettings";
import { typeInto } from "./ui/testing";

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

describe("NetworkSettings", () => {
  it("nests the manual server card and keeps the proxy actions in the mode card", async () => {
    mockBackend();
    render(<NetworkSettings />);

    expect(await screen.findByText("代理模式")).toBeInTheDocument();
    expect(screen.queryByText("代理服务器")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "手动" }));

    const serverHeadline = await screen.findByText("代理服务器");
    const nestedCard = serverHeadline.closest("m-card");
    expect(nestedCard).not.toBeNull();
    const outerCard = nestedCard?.parentElement?.closest("m-card");
    expect(outerCard).not.toBeNull();

    expect(
      within(outerCard as HTMLElement).getByRole("button", { name: /保存/ }),
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
    toggle.dispatchEvent(new CustomEvent("change", { detail: { checked: true } }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_proxy_settings", {
        settings: expect.objectContaining({ campusThroughProxy: true }),
      });
    });
  });

  it("tests the current proxy configuration", async () => {
    mockBackend();
    render(<NetworkSettings />);

    fireEvent.click(await screen.findByRole("button", { name: "测试连接" }));

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
    expect(screen.getByRole("radio", { name: "手动" })).toBeChecked();
    const bypass = screen.getByLabelText("校内网也走代理") as HTMLElement & {
      checked: boolean;
    };
    expect(bypass.checked).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: /恢复默认/ }));

    await waitFor(() => expect(screen.getByRole("radio", { name: "自动" })).toBeChecked());
    expect(bypass.checked).toBe(true);
  });
});
