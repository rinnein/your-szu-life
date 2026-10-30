/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { typeInto } from "./ui/testing";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import AiConnectionSettings from "./AiConnectionSettings";
import { toast } from "./ui";
import type { StepNextAction } from "./step-next-action";
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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("updates both providers without sending or exposing saved keys", async () => {
  const success = vi.spyOn(toast, "success").mockReturnValue("toast-id");
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
  await vi.waitFor(() => expect(success).toHaveBeenCalledWith("AI 连接配置已保存"));
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

it("reports connection save failures through the snackbar", async () => {
  const danger = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
  render(<AiConnectionSettings />);
  await screen.findByLabelText("主模型 API Key");
  vi.mocked(invoke).mockRejectedValueOnce("连接配置保存失败");
  fireEvent.click(screen.getByRole("button", { name: "保存 AI 连接" }));
  await vi.waitFor(() =>
    expect(danger).toHaveBeenCalledWith("保存 AI 连接失败", {
      description: "连接配置保存失败",
    }),
  );
});

it("registers 保存并继续 for the onboarding footer and completes with a configured main model", async () => {
  const onComplete = vi.fn();
  const actions: Array<StepNextAction | null> = [];
  render(
    <AiConnectionSettings
      onboarding
      onComplete={onComplete}
      registerNext={(action) => actions.push(action)}
    />,
  );

  await screen.findByLabelText("主模型 API 地址");
  expect(screen.queryByLabelText("Jev API Key")).not.toBeInTheDocument();
  await waitFor(() => expect(actions[actions.length - 1]?.label).toBe("保存并继续"));

  await actions[actions.length - 1]?.run();
  await waitFor(() => expect(onComplete).toHaveBeenCalledTimes(1));
});

it("stays on the onboarding step when the main model is incomplete", async () => {
  const onComplete = vi.fn();
  const info = vi.spyOn(toast, "info").mockReturnValue("toast-id");
  const actions: Array<StepNextAction | null> = [];
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "save_uooc_ai_connections") {
      return {
        main: { baseUrl: "", hasKey: false },
        jev: { baseUrl: "https://api.typesafe.ai/v1", hasKey: false },
      } satisfies AiConnections;
    }
    return connections;
  });

  render(
    <AiConnectionSettings
      onboarding
      onComplete={onComplete}
      registerNext={(action) => actions.push(action)}
    />,
  );

  await screen.findByLabelText("主模型 API 地址");
  await waitFor(() => expect(actions[actions.length - 1]?.label).toBe("保存并继续"));

  await actions[actions.length - 1]?.run();
  await waitFor(() =>
    expect(info).toHaveBeenCalledWith("主模型尚未配置完整；可以继续填写，或跳过此步骤"),
  );
  expect(onComplete).not.toHaveBeenCalled();
});
