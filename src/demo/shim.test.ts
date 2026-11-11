/** @vitest-environment jsdom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { demoActive, setDemoActive } from "./demo-state";
import { invoke } from "./shim";

const internalsInvoke = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  setDemoActive(false);
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: { invoke: internalsInvoke },
  });
});

afterEach(() => {
  setDemoActive(false);
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe("demo shim", () => {
  it("passes commands through while demo mode is off", async () => {
    internalsInvoke.mockResolvedValue({ Unauthenticated: null });
    await expect(invoke("get_auth_state")).resolves.toEqual({ Unauthenticated: null });
    expect(internalsInvoke).toHaveBeenCalledWith("get_auth_state", undefined, undefined);
  });

  it("serves mock auth data while demo mode is on", async () => {
    setDemoActive(true);
    const state = await invoke<{ Authenticated?: { studentId?: string } }>("get_auth_state");
    expect(state.Authenticated?.studentId).toBe("20250001");
    expect(internalsInvoke).not.toHaveBeenCalled();
  });

  it("still passes unknown commands through while demo mode is on", async () => {
    setDemoActive(true);
    internalsInvoke.mockResolvedValue("ok");
    await expect(invoke("take_port_notices")).resolves.toBe("ok");
    expect(internalsInvoke).toHaveBeenCalledWith("take_port_notices", undefined, undefined);
  });

  it("blocks real login entry points while demo mode is on", async () => {
    setDemoActive(true);
    await expect(invoke("begin_authentication")).rejects.toThrow("演示模式下不可用");
    expect(internalsInvoke).not.toHaveBeenCalled();
  });

  it("keeps logout a no-op so real credentials are safe", async () => {
    setDemoActive(true);
    await expect(invoke("logout")).resolves.toBeUndefined();
    expect(internalsInvoke).not.toHaveBeenCalled();
  });

  it("routes unknown commands to the real backend without an internals bridge", async () => {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    await expect(invoke("get_auth_state")).rejects.toThrow("Tauri IPC 不可用");
  });

  it("updates the local mirror flag when set_demo_mode succeeds", async () => {
    internalsInvoke.mockResolvedValue({ enabled: true, demoMode: true, logCount: 0 });
    await invoke("set_demo_mode", { enabled: true });
    expect(demoActive()).toBe(true);
    // 之后的登录数据读取都走 mock，不再触达 internals。
    const state = await invoke<{ Authenticated?: { studentId?: string } }>("get_auth_state");
    expect(state.Authenticated?.studentId).toBe("20250001");
  });

  it("reverts the mirror flag when set_demo_mode fails", async () => {
    internalsInvoke.mockRejectedValue(new Error("backend down"));
    await expect(invoke("set_demo_mode", { enabled: true })).rejects.toThrow("backend down");
    expect(demoActive()).toBe(false);
  });
});
