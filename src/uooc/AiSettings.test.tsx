/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "@heroui/react";
import { AiSettings } from "./AiSettings";
import type { AiConfig } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const config: AiConfig = {
  main: { baseUrl: "https://example.org/v1", model: "", hasKey: true },
  jev: { baseUrl: "https://api.typesafe.ai/v1", model: "jev-latest", hasKey: false },
  jevEnabled: false,
  confidence: 0.8,
  defaultMode: "supervised",
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
  vi.mocked(invoke).mockImplementation(async (name) =>
    name === "list_uooc_ai_models" ? ["m1", "m2"] : config,
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it("keeps connection fields out of UOOC and saves only model preferences", async () => {
  const successSpy = vi.spyOn(toast, "success");
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  await screen.findByRole("combobox", { name: "主模型名称" });
  expect(screen.queryByLabelText("主模型 API Key")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("主模型 API 地址")).not.toBeInTheDocument();
  fireEvent.change(screen.getByRole("combobox", { name: "主模型名称" }), {
    target: { value: "custom-model" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存运行设置" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_ai_preferences", {
      preferences: {
        mainModel: "custom-model",
        jevModel: "jev-latest",
        jevEnabled: false,
        confidence: 0.8,
        defaultMode: "supervised",
        batchSize: 5,
        reasoningEffort: "default",
        includeReasoning: true,
        webSearch: true,
      },
    }),
  );
  await waitFor(() => expect(successSpy).toHaveBeenCalledWith("配置已保存"));
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});
it("loads model autocomplete through the configured native provider", async () => {
  const successSpy = vi.spyOn(toast, "success");
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  await screen.findByRole("combobox", { name: "主模型名称" });
  fireEvent.click(screen.getByRole("button", { name: "加载主模型列表" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("list_uooc_ai_models", { provider: "main" }),
  );
  await waitFor(() => expect(successSpy).toHaveBeenCalledWith("已加载 2 个模型，选择后保存配置"));
});

it("opens the global AI connection settings from the model settings", async () => {
  const onOpenAiSettings = vi.fn();
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={onOpenAiSettings} />);
  await screen.findByRole("combobox", { name: "主模型名称" });
  fireEvent.click(screen.getByRole("button", { name: "前往 AI 连接设置" }));
  expect(onOpenAiSettings).toHaveBeenCalledOnce();
});

it("shows UOOC AI save failures via toast.danger instead of form alerts", async () => {
  const dangerSpy = vi.spyOn(toast, "danger");
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  await screen.findByRole("combobox", { name: "主模型名称" });
  vi.mocked(invoke).mockRejectedValueOnce("运行设置保存失败");
  fireEvent.click(screen.getByRole("button", { name: "保存运行设置" }));
  await waitFor(() =>
    expect(dangerSpy).toHaveBeenCalledWith("保存配置失败", {
      description: "运行设置保存失败",
    }),
  );
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

it("shows alert when reading initial AI config fails", async () => {
  vi.mocked(invoke).mockRejectedValueOnce("无法读取 AI 配置");
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("无法读取 AI 配置");
});
