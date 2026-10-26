/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { typeInto } from "./ui/testing";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import AiConnectionSettings from "./AiConnectionSettings";
import type { AiConnections } from "./uooc/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const connections: AiConnections = {
  main: { baseUrl: "https://example.org/v1", hasKey: true },
  jev: { baseUrl: "https://api.typesafe.ai/v1", hasKey: false },
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.getAnimations = () => [];
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "get_uooc_ai_connections" ? connections : connections,
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("updates both providers without sending or exposing saved keys", async () => {
  render(<AiConnectionSettings />);
  const mainUrl = await screen.findByLabelText("主模型 API 地址");
  expect(screen.getByLabelText("主模型 API Key")).toHaveValue("");
  typeInto(mainUrl, "http://192.168.1.20:8000/v1");
  typeInto(screen.getByLabelText("Jev API Key"), "jev-new-key");
  await waitFor(() => expect(screen.getByLabelText("Jev API Key")).toHaveValue("jev-new-key"));
  // 受控输入需要一次渲染才反映到状态，点保存前先等它落地。
  // 查询落到 shadow 内的真实输入控件上：受控值反映在 value 属性/属性值上，
  // 用 toHaveValue 读当前输入值，而不是宿主的 value 属性。
  await waitFor(() =>
    expect(screen.getByLabelText("主模型 API 地址")).toHaveValue("http://192.168.1.20:8000/v1"),
  );
  fireEvent.click(screen.getByRole("button", { name: "保存 AI 连接" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_ai_connections", {
      connections: {
        main: { baseUrl: "http://192.168.1.20:8000/v1", apiKey: null, clearKey: false },
        jev: {
          baseUrl: "https://api.typesafe.ai/v1",
          apiKey: "jev-new-key",
          clearKey: false,
        },
      },
    }),
  );
  expect(await screen.findByRole("status")).toHaveTextContent("AI 连接配置已保存");
});

it("supports explicit key removal", async () => {
  render(<AiConnectionSettings />);
  await screen.findByLabelText("主模型 API Key");
  fireEvent.click(screen.getByRole("checkbox", { name: "删除已保存的主模型密钥" }));
  fireEvent.click(screen.getByRole("button", { name: "保存 AI 连接" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith(
      "save_uooc_ai_connections",
      expect.objectContaining({
        connections: expect.objectContaining({
          main: expect.objectContaining({ apiKey: null, clearKey: true }),
        }),
      }),
    ),
  );
});

it("shows connection load failures in a visible alert", async () => {
  vi.mocked(invoke).mockRejectedValueOnce("连接配置读取失败");
  render(<AiConnectionSettings />);
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("连接配置读取失败");
});

it("shows connection save failures in a visible alert above the fields", async () => {
  render(<AiConnectionSettings />);
  const key = await screen.findByLabelText("主模型 API Key");
  vi.mocked(invoke).mockRejectedValueOnce("连接配置保存失败");
  fireEvent.click(screen.getByRole("button", { name: "保存 AI 连接" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("连接配置保存失败");
  expect(alert.compareDocumentPosition(key) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});
