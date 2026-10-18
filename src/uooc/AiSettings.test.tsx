/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
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
      },
    }),
  );
});
it("loads model autocomplete through the configured native provider", async () => {
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  await screen.findByRole("combobox", { name: "主模型名称" });
  fireEvent.click(screen.getByRole("button", { name: "加载主模型列表" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("list_uooc_ai_models", { provider: "main" }),
  );
  expect(await screen.findByText("已加载 2 个模型，选择后保存配置")).toBeInTheDocument();
});

it("opens the global AI connection settings from the model settings", async () => {
  const onOpenAiSettings = vi.fn();
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={onOpenAiSettings} />);
  await screen.findByRole("combobox", { name: "主模型名称" });
  fireEvent.click(screen.getByRole("button", { name: "前往 AI 连接设置" }));
  expect(onOpenAiSettings).toHaveBeenCalledOnce();
});

it("shows UOOC AI save failures in a visible alert above the form", async () => {
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  await screen.findByRole("combobox", { name: "主模型名称" });
  vi.mocked(invoke).mockRejectedValueOnce("运行设置保存失败");
  fireEvent.click(screen.getByRole("button", { name: "保存运行设置" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("运行设置保存失败");
  expect(
    alert.compareDocumentPosition(screen.getByRole("combobox", { name: "主模型名称" })) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
});
