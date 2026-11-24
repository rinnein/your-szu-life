/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import StorageSettings from "./StorageSettings";
import { hostOf } from "./ui/testing";
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

  it("offers a retry when the snapshot cannot be read", async () => {
    let attempts = 0;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") {
        attempts += 1;
        if (attempts === 1) throw "读取保存目录失败";
        return desktopState;
      }
      return undefined;
    });

    render(<StorageSettings />);

    expect(await screen.findByText("无法读取保存目录")).toBeInTheDocument();
    expect(screen.getByText("读取保存目录失败")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    expect(await screen.findByText("/home/me/Documents/泥深")).toBeInTheDocument();
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

  it("asks for confirmation before falling back to the default directory", async () => {
    let current = pickedState;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return current;
      if (command === "use_default_file_manager_directory") {
        return {
          selection: { kind: "default" },
          selected: "/home/me/Documents/泥深",
          selectedWasEmpty: true,
          base: "/home/me/Documents/泥深",
          modules: [{ id: "jxxk", label: "督导巡课录像", path: "/home/me/Documents/泥深/jxxk" }],
          notice: null,
        };
      }
      if (command === "save_file_manager_directory") {
        current = desktopState;
        return desktopState;
      }
      return undefined;
    });

    render(<StorageSettings />);
    await screen.findByText("/home/me/Videos/泥深");

    const button = screen.getByRole("button", { name: "使用默认目录" });
    // 图标旁要有可见文字，而不是只有一个图标。
    expect(hostOf(button)).toHaveTextContent("默认");
    fireEvent.click(button);

    expect(await screen.findByText("确认保存目录")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_file_manager_directory", {
        selection: { kind: "default" },
      }),
    );
  });

  it("guides Android users to authorize the default directory through SAF", async () => {
    const android: FileManagerState = {
      platform: "android",
      defaultBase: "/storage/emulated/0/Download/泥深",
      config: null,
      base: "/storage/emulated/0/Download/泥深",
      usingDefault: true,
      access: { state: "denied", detail: "尚未选择保存目录" },
      modules: [
        { id: "jxxk", label: "督导巡课录像", path: "/storage/emulated/0/Download/泥深/jxxk" },
      ],
      canUseDefault: false,
    };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_file_manager_state") return android;
      if (command === "pick_file_manager_directory") return null;
      return undefined;
    });

    render(<StorageSettings />);
    await screen.findByText("/storage/emulated/0/Download/泥深");

    fireEvent.click(screen.getByRole("button", { name: "使用默认目录" }));

    // 额外一步：先说明 SAF 授权，再由系统选择器完成；不直接落库。
    expect(await screen.findByText("使用默认目录")).toBeInTheDocument();
    expect(screen.getByText(/应用不会申请任何存储权限/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("use_default_file_manager_directory");

    fireEvent.click(screen.getByRole("button", { name: "去选择目录" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pick_file_manager_directory"));
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
    // Android 也提供「默认」入口，但它只负责引导用户去系统选择器授权。
    expect(screen.getByRole("button", { name: "使用默认目录" })).toBeInTheDocument();
  });
});
