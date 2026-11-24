/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { typeInto } from "../ui/testing";
import AiSettingsPage from "./AiSettingsPage";
import type { AiConfigView, AiModelConfig } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function model(id: string, name = id): AiModelConfig {
  return {
    id,
    name,
    url: "https://example.org/v1/chat/completions",
    toolCalling: true,
    vision: false,
    maxInputTokens: 119808,
    maxOutputTokens: 8192,
  };
}

const config: AiConfigView = {
  providers: [
    {
      id: "p",
      vendor: "openai",
      name: "Test Provider",
      protocol: "openAiCompatible",
      baseUrl: "https://example.org/v1",
      models: [model("test-model", "Test Model")],
      hasKey: true,
      builtin: false,
    },
  ],
  default: { providerId: "p", model: "test-model" },
};

const difyConfig: AiConfigView = {
  providers: [
    {
      id: "dify",
      vendor: "dify",
      name: "UOOC 智能小优",
      protocol: "difyChat",
      baseUrl: "https://dify.example.com/chatbot/abc",
      models: [
        {
          id: "chatbot",
          name: "dify-chatbot-abc",
          url: "https://dify.example.com/chatbot/abc",
          toolCalling: false,
          vision: false,
          maxInputTokens: 128000,
          maxOutputTokens: 8192,
        },
      ],
      hasKey: false,
      builtin: true,
      appCode: "abc",
    },
  ],
  default: { providerId: "dify", model: "chatbot" },
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
  vi.mocked(invoke).mockResolvedValue(config);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders providers and the default model without exposing saved keys", async () => {
  render(<AiSettingsPage />);
  expect((await screen.findAllByText("Test Provider")).length).toBeGreaterThan(0);
  expect((await screen.findAllByText("Test Model")).length).toBeGreaterThan(0);
  expect(screen.queryByDisplayValue("secret-key")).not.toBeInTheDocument();
  expect(screen.getByText("默认")).toBeInTheDocument();
});

it("detects a new provider and saves its model with capabilities", async () => {
  const empty: AiConfigView = { providers: [], default: null };
  const saved: AiConfigView = {
    providers: [
      {
        id: "new",
        vendor: "openai",
        name: "Test Provider",
        protocol: "openAiCompatible",
        baseUrl: "https://example.org/v1",
        models: [model("m1", "M1")],
        hasKey: true,
        builtin: false,
      },
    ],
    default: { providerId: "new", model: "m1" },
  };
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ai_config") return empty;
    if (command === "detect_ai_provider") {
      return {
        protocol: "openAiCompatible",
        vendor: "openai",
        baseUrl: "https://example.org/v1",
        models: [model("m1", "M1")],
        verified: true,
      };
    }
    if (command === "upsert_ai_provider") return saved;
    return undefined;
  });

  render(<AiSettingsPage />);
  fireEvent.click((await screen.findAllByRole("button", { name: "添加提供商" }))[0]);
  await screen.findByText("添加 AI 提供商");
  typeInto(screen.getByLabelText("API 地址"), "https://example.org/v1");
  typeInto(screen.getByLabelText("API Key"), "key");
  fireEvent.click(screen.getByRole("button", { name: "自动探测供应商与模型" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("detect_ai_provider", {
      input: {
        baseUrl: "https://example.org/v1",
        apiKey: "key",
        providerId: null,
        protocol: null,
      },
    }),
  );
  expect(await screen.findByText(/凭据已验证/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "保存提供商" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("upsert_ai_provider", {
      input: expect.objectContaining({
        name: "",
        vendor: "openai",
        baseUrl: "https://example.org/v1",
        apiKey: "key",
        models: [expect.objectContaining({ id: "m1", toolCalling: true })],
        model: "m1",
      }),
    }),
  );
  expect((await screen.findAllByText("Test Provider")).length).toBeGreaterThan(0);
});

it("does not flag a built-in Dify public chat provider and hides its actions", async () => {
  vi.mocked(invoke).mockResolvedValue(difyConfig);
  render(<AiSettingsPage />);
  expect((await screen.findAllByText("UOOC 智能小优")).length).toBeGreaterThan(0);
  expect(screen.queryByText("未设置 Key")).not.toBeInTheDocument();
  expect(screen.getByText("内置 · 锁定")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "编辑" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "删除" })).not.toBeInTheDocument();
  expect(screen.getAllByText(/dify-chatbot-abc/).length).toBeGreaterThan(0);
});

it("offers a reset action when the config cannot be read", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ai_config") {
      throw new Error("AI 配置格式无效，请重新配置提供商");
    }
    if (command === "reset_ai_config") return difyConfig;
    return undefined;
  });
  render(<AiSettingsPage />);
  expect(await screen.findByText("无法读取 AI 配置")).toBeInTheDocument();
  expect(screen.getByText("AI 配置格式无效，请重新配置提供商")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "重置 AI 配置" }));
  expect(await screen.findByText("重置 AI 配置？")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "重置" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("reset_ai_config"));
  expect((await screen.findAllByText("UOOC 智能小优")).length).toBeGreaterThan(0);
});

it("shows a persistent warning when detection fails and keeps manual vendor selection", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ai_config") return { providers: [], default: null };
    if (command === "detect_ai_provider") throw new Error("connection refused");
    return undefined;
  });
  render(<AiSettingsPage />);
  fireEvent.click((await screen.findAllByRole("button", { name: "添加提供商" }))[0]);
  await screen.findByText("添加 AI 提供商");
  typeInto(screen.getByLabelText("API 地址"), "https://example.org/v1");
  fireEvent.click(screen.getByRole("button", { name: "自动探测供应商与模型" }));
  expect(await screen.findByText("自动探测失败")).toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "模型供应商" })).toBeInTheDocument();
});

it("keeps the saved key when editing and supports explicit removal", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ai_config") return config;
    if (command === "upsert_ai_provider") return config;
    return undefined;
  });
  render(<AiSettingsPage />);
  await screen.findAllByText("Test Provider");

  fireEvent.click(screen.getByRole("button", { name: "编辑" }));
  await screen.findByText("编辑 Test Provider");
  fireEvent.click(screen.getByRole("button", { name: "保存提供商" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("upsert_ai_provider", {
      input: expect.objectContaining({ id: "p", apiKey: null, clearKey: false }),
    }),
  );

  fireEvent.click(await screen.findByRole("button", { name: "编辑" }));
  await screen.findByText("编辑 Test Provider");
  fireEvent.click(screen.getByRole("checkbox", { name: "删除已保存的密钥" }));
  fireEvent.click(screen.getByRole("button", { name: "保存提供商" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("upsert_ai_provider", {
      input: expect.objectContaining({ id: "p", apiKey: null, clearKey: true }),
    }),
  );
});

it("confirms deletion and clears the default provider", async () => {
  const empty: AiConfigView = { providers: [], default: null };
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ai_config") return config;
    if (command === "remove_ai_provider") return empty;
    return undefined;
  });
  render(<AiSettingsPage />);
  await screen.findAllByText("Test Provider");
  fireEvent.click(screen.getByRole("button", { name: "删除" }));
  expect(await screen.findByText("删除 AI 提供商？")).toBeInTheDocument();
  const deleteButtons = screen.getAllByRole("button", { name: "删除" });
  fireEvent.click(deleteButtons[deleteButtons.length - 1]);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("remove_ai_provider", { providerId: "p" }),
  );
  expect(await screen.findByText("尚未配置 AI 提供商")).toBeInTheDocument();
});
