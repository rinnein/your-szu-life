/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "../../ui";
import { AiSettings } from "./AiSettings";
import type { UoocAiConfig } from "../state/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const config: UoocAiConfig = {
  provider: {
    id: "p",
    name: "Provider",
    vendor: "openai",
    protocol: "openAiCompatible",
    baseUrl: "https://example.org/v1",
    model: "m1",
    modelName: "M1",
    hasKey: true,
    builtin: false,
  },
  batchSize: 5,
  reasoningEffort: "default",
  includeReasoning: true,
  webSearch: true,
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
  vi.mocked(invoke).mockResolvedValue(config);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("keeps connection fields out of UOOC and saves only runtime preferences", async () => {
  const successSpy = vi.spyOn(toast, "success");
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  expect(await screen.findByText("Provider · M1")).toBeInTheDocument();
  expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("API 地址")).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "保存运行设置" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_ai_preferences", {
      preferences: {
        batchSize: 5,
        reasoningEffort: "default",
        includeReasoning: true,
        webSearch: true,
        defaultMode: "supervised",
      },
    }),
  );
  await waitFor(() => expect(successSpy).toHaveBeenCalledWith("测验 AI 设置已保存"));
});

it("opens the global AI configuration from the default model row", async () => {
  const onOpenAiSettings = vi.fn();
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={onOpenAiSettings} />);
  await screen.findByText("Provider · M1");
  fireEvent.click(screen.getByRole("button", { name: "前往 AI 配置" }));
  expect(onOpenAiSettings).toHaveBeenCalledOnce();
});

it("shows a persistent banner when no default model is configured", async () => {
  vi.mocked(invoke).mockResolvedValue({ ...config, provider: null });
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  expect(await screen.findByText("尚未配置默认模型")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "前往 AI 配置" })).toBeInTheDocument();
});

it("shows UOOC AI save failures via toast.danger instead of form alerts", async () => {
  const dangerSpy = vi.spyOn(toast, "danger");
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  await screen.findByText("Provider · M1");
  vi.mocked(invoke).mockRejectedValueOnce("运行设置保存失败");
  fireEvent.click(screen.getByRole("button", { name: "保存运行设置" }));
  await waitFor(() =>
    expect(dangerSpy).toHaveBeenCalledWith("保存测验 AI 设置失败", {
      description: "运行设置保存失败",
    }),
  );
  expect(document.querySelector(".banner")).toBeNull();
});

it("shows an alert when reading the initial AI configuration fails", async () => {
  vi.mocked(invoke).mockRejectedValueOnce("无法读取 AI 配置");
  render(<AiSettings onMode={vi.fn()} onOpenAiSettings={vi.fn()} />);
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("无法读取 AI 配置");
});
