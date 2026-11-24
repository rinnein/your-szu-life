/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { FileManagerState } from "../file-manager/api";
import type { StepNextAction } from "../step-next-action";
import { StorageStep } from "./StorageStep";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const desktopState: FileManagerState = {
  platform: "linux",
  defaultBase: "/home/me/Documents/泥深",
  config: null,
  base: "/home/me/Documents/泥深",
  usingDefault: true,
  access: { state: "notApplicable", detail: "桌面端使用系统文件系统权限" },
  modules: [{ id: "jxxk", label: "督导巡课录像", path: "/home/me/Documents/泥深/jxxk" }],
  canUseDefault: true,
};

const androidState: FileManagerState = {
  platform: "android",
  defaultBase: "/storage/emulated/0/Download/泥深",
  config: null,
  base: "/storage/emulated/0/Download/泥深",
  usingDefault: true,
  access: { state: "denied", detail: "尚未选择保存目录" },
  modules: [{ id: "jxxk", label: "督导巡课录像", path: "/storage/emulated/0/Download/泥深/jxxk" }],
  canUseDefault: false,
};

const defaultPreview = {
  selection: { kind: "default" },
  selected: "/home/me/Documents/泥深",
  selectedWasEmpty: true,
  base: "/home/me/Documents/泥深",
  modules: [{ id: "jxxk", label: "督导巡课录像", path: "/home/me/Documents/泥深/jxxk" }],
  notice: null,
};

function Harness({ onComplete = vi.fn() }: { onComplete?: () => void | Promise<void> }) {
  const [action, setAction] = useState<StepNextAction | null>(null);
  return (
    <>
      <StorageStep onComplete={onComplete} onSkip={vi.fn()} registerNext={setAction} />
      {action && (
        <button type="button" disabled={action.disabled} onClick={() => void action.run()}>
          {action.label}
        </button>
      )}
    </>
  );
}

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
  Element.prototype.scrollIntoView = vi.fn();
  vi.mocked(listen).mockResolvedValue(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("StorageStep", () => {
  it("treats 下一步 without a choice as the default directory and confirms it", async () => {
    const onComplete = vi.fn();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return desktopState;
      if (command === "use_default_file_manager_directory") return defaultPreview;
      if (command === "save_file_manager_directory") return desktopState;
      return undefined;
    });

    render(<Harness onComplete={onComplete} />);
    await screen.findByText("/home/me/Documents/泥深");

    fireEvent.click(await screen.findByRole("button", { name: "下一步" }));

    // 与直接点「默认」一致：先弹出确认框，而不是直接打开目录选择器。
    expect(await screen.findByText("确认保存目录")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("use_default_file_manager_directory");
    expect(invoke).not.toHaveBeenCalledWith("pick_file_manager_directory");

    fireEvent.click(screen.getByRole("button", { name: "确认" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_file_manager_directory", {
        selection: { kind: "default" },
      }),
    );
    await waitFor(() => expect(onComplete).toHaveBeenCalledTimes(1));
  });

  it("guides Android users to the system picker instead of saving directly", async () => {
    const onComplete = vi.fn();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return androidState;
      if (command === "pick_file_manager_directory") return null;
      return undefined;
    });

    render(<Harness onComplete={onComplete} />);
    await screen.findByText("/storage/emulated/0/Download/泥深");

    fireEvent.click(await screen.findByRole("button", { name: "下一步" }));

    expect(await screen.findByText("使用默认目录")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("use_default_file_manager_directory");
    expect(invoke).not.toHaveBeenCalledWith("pick_file_manager_directory");

    fireEvent.click(screen.getByRole("button", { name: "去选择目录" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pick_file_manager_directory"));
    expect(onComplete).not.toHaveBeenCalled();
  });

  it("advances directly once a directory is already configured", async () => {
    const onComplete = vi.fn();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") {
        return {
          ...desktopState,
          config: {
            selection: { kind: "filesystem", path: "/home/me/Videos" },
            relativeBase: "",
            displayRoot: "/home/me/Videos/泥深",
            selected: "/home/me/Videos",
            viaDefault: false,
            chosenAt: 1,
          },
          base: "/home/me/Videos/泥深",
          usingDefault: false,
        } satisfies FileManagerState;
      }
      return undefined;
    });

    render(<Harness onComplete={onComplete} />);
    await screen.findByText("/home/me/Videos/泥深");

    fireEvent.click(await screen.findByRole("button", { name: "保存并继续" }));

    await waitFor(() => expect(onComplete).toHaveBeenCalledTimes(1));
    expect(invoke).not.toHaveBeenCalledWith("use_default_file_manager_directory");
  });
});
