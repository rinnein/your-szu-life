/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import StorageSettings from "./StorageSettings";
import type { FileManagerState } from "./file-manager/api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

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

const pickedState: FileManagerState = {
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
  modules: [{ id: "jxxk", label: "督导巡课录像", path: "/home/me/Videos/泥深/jxxk" }],
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
  Element.prototype.scrollIntoView = vi.fn();
  vi.mocked(listen).mockResolvedValue(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("StorageSettings", () => {
  it("shows the effective directory and every module subdirectory", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return desktopState;
      return undefined;
    });

    render(<StorageSettings />);

    expect(await screen.findByText("/home/me/Documents/泥深")).toBeInTheDocument();
    expect(screen.getByText("督导巡课录像")).toBeInTheDocument();
    expect(screen.getByText("/home/me/Documents/泥深/jxxk")).toBeInTheDocument();
    expect(screen.getByText(/当前使用平台默认目录/)).toBeInTheDocument();
  });

  it("asks for confirmation and saves the resolved directory", async () => {
    let current = desktopState;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return current;
      if (command === "pick_file_manager_directory") {
        return {
          selection: { kind: "filesystem", path: "/home/me/Videos" },
          selected: "/home/me/Videos",
          selectedWasEmpty: false,
          base: "/home/me/Videos/泥深",
          modules: [{ id: "jxxk", label: "督导巡课录像", path: "/home/me/Videos/泥深/jxxk" }],
          notice: "所选目录已有其它内容，文件将统一放在其中的「泥深」子目录。",
        };
      }
      if (command === "save_file_manager_directory") {
        current = pickedState;
        return pickedState;
      }
      return undefined;
    });

    render(<StorageSettings />);
    await screen.findByText("/home/me/Documents/泥深");

    fireEvent.click(screen.getByRole("button", { name: "选择目录" }));
    expect(await screen.findByText("确认保存目录")).toBeInTheDocument();
    expect(screen.getByText("/home/me/Videos/泥深")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确认" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_file_manager_directory", {
        selection: { kind: "filesystem", path: "/home/me/Videos" },
      }),
    );
    expect(await screen.findByText("/home/me/Videos/泥深/jxxk")).toBeInTheDocument();
  });

  it("releases the granted folder when the confirmation is cancelled", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return desktopState;
      if (command === "pick_file_manager_directory") {
        return {
          selection: { kind: "androidTree", uri: "content://tree/primary%3ADownload" },
          selected: "/storage/emulated/0/Download",
          selectedWasEmpty: true,
          base: "/storage/emulated/0/Download",
          modules: [],
          notice: null,
        };
      }
      return undefined;
    });

    render(<StorageSettings />);
    await screen.findByText("/home/me/Documents/泥深");

    fireEvent.click(screen.getByRole("button", { name: "选择目录" }));
    await screen.findByText("确认保存目录");
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("release_file_manager_pick", {
        selection: { kind: "androidTree", uri: "content://tree/primary%3ADownload" },
      }),
    );
    expect(screen.queryByText("确认保存目录")).not.toBeInTheDocument();
  });

  it("warns when the Android folder authorization is gone", async () => {
    const broken: FileManagerState = {
      ...pickedState,
      platform: "android",
      access: { state: "denied", detail: "系统文件夹授权已失效，请重新选择保存目录" },
      canUseDefault: false,
    };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return broken;
      return undefined;
    });

    render(<StorageSettings />);

    expect(await screen.findByText("保存目录不可用")).toBeInTheDocument();
    expect(screen.getAllByText("系统文件夹授权已失效，请重新选择保存目录").length).toBeGreaterThan(
      0,
    );
    // Android 不提供「恢复默认目录」：授权必须由系统选择器给出。
    expect(screen.queryByRole("button", { name: "使用默认目录" })).not.toBeInTheDocument();
  });
});
