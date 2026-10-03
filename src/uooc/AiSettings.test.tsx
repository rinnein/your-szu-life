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
  defaultMode: "approval",
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
it("never loads the stored key into the form and preserves it when left empty", async () => {
  render(<AiSettings onMode={vi.fn()} />);
  const key = await screen.findByLabelText("主模型 API Key");
  expect(key).toHaveValue("");
  fireEvent.change(screen.getByRole("combobox", { name: "主模型名称" }), {
    target: { value: "custom-model" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存 AI 配置" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_ai_config", {
      config: expect.objectContaining({
        main: {
          baseUrl: "https://example.org/v1",
          model: "custom-model",
          apiKey: null,
          clearKey: false,
        },
      }),
    }),
  );
});
it("loads model autocomplete through the native provider and supports explicit key deletion", async () => {
  render(<AiSettings onMode={vi.fn()} />);
  await screen.findByLabelText("主模型 API Key");
  fireEvent.click(screen.getByRole("checkbox", { name: "删除已保存的主模型密钥" }));
  fireEvent.click(screen.getByRole("button", { name: "保存并加载主模型列表" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("list_uooc_ai_models", { provider: "main" }),
  );
  expect(invoke).toHaveBeenCalledWith("save_uooc_ai_config", {
    config: expect.objectContaining({
      main: expect.objectContaining({ clearKey: true, apiKey: null }),
    }),
  });
  expect(await screen.findByText("已加载 2 个模型，选择后保存配置")).toBeInTheDocument();
});
