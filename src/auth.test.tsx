/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "@heroui/react";
import { AuthProvider } from "./auth";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("AuthProvider global warnings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(() => undefined);
  });

  afterEach(() => {
    cleanup();
  });

  it("triggers toast.warning when dorm login failure notice is consumed", async () => {
    const warningSpy = vi.spyOn(toast, "warning").mockReturnValue("toast-id");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "consume_dorm_login_failure_notice") {
        return { message: "密码错误" };
      }
      if (command === "get_auth_state") return "unauthenticated";
      return null;
    });

    render(
      <AuthProvider>
        <div>child</div>
      </AuthProvider>,
    );

    await waitFor(() => {
      expect(warningSpy).toHaveBeenCalledWith("校园网自动登录失败：密码错误");
    });
  });

  it("triggers toast.warning when staff login failure notice is consumed", async () => {
    const warningSpy = vi.spyOn(toast, "warning").mockReturnValue("toast-id");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "consume_staff_login_failure_notice") {
        return { message: "教工网会话超时" };
      }
      if (command === "get_auth_state") return "unauthenticated";
      return null;
    });

    render(
      <AuthProvider>
        <div>child</div>
      </AuthProvider>,
    );

    await waitFor(() => {
      expect(warningSpy).toHaveBeenCalledWith("校园网自动登录失败：教工网会话超时");
    });
  });
});
