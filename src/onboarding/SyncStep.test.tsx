/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { SyncStep } from "./SyncStep";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SyncStep", () => {
  it("enables sync, triggers a pass and then completes the step", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_mesh_status") return { enabled: false, running: false, error: null };
      if (command === "set_mesh_enabled") return { enabled: true, running: true, error: null };
      if (command === "trigger_sync")
        return {
          enabled: true,
          campusAccess: false,
          running: true,
          summary: {
            peers: 1,
            applied: 2,
            pushed: 0,
            bankRows: 3,
            errors: [],
          },
          items: [],
        };
      return undefined;
    });

    let action: { label: string; run: () => void | Promise<void> } | null = null;
    const onComplete = vi.fn();

    render(
      <SyncStep
        onComplete={onComplete}
        onSkip={() => undefined}
        registerNext={(next) => {
          action = next;
        }}
      />,
    );

    await waitFor(() => expect(action?.label).toBe("开启并立即同步"));
    await action!.run();

    expect(invoke).toHaveBeenCalledWith("set_mesh_enabled", { enabled: true });
    expect(invoke).toHaveBeenCalledWith("trigger_sync");
    await waitFor(() => expect(onComplete).toHaveBeenCalled());
    expect(await screen.findByText("同步完成")).toBeInTheDocument();
  });

  it("can complete without enabling sync", async () => {
    vi.mocked(invoke).mockResolvedValue({ enabled: false, running: false, error: null });
    let action: { label: string; run: () => void | Promise<void> } | null = null;
    const onComplete = vi.fn();

    render(
      <SyncStep
        onComplete={onComplete}
        onSkip={() => undefined}
        registerNext={(next) => {
          action = next;
        }}
      />,
    );

    await waitFor(() => expect(action?.label).toBe("开启并立即同步"));
    // 关闭开关后主操作变为「不开启，继续」。
    const toggle = screen.getByLabelText("开启自动同步");
    toggle.dispatchEvent(new CustomEvent("change", { detail: { checked: false } }));
    await waitFor(() => expect(action?.label).toBe("不开启，继续"));

    await action!.run();
    expect(invoke).not.toHaveBeenCalledWith("set_mesh_enabled", { enabled: true });
    await waitFor(() => expect(onComplete).toHaveBeenCalled());
  });
});
