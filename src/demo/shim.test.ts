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

  it("serves a ready app-content snapshot so the homepage notice bar is populated", async () => {
    setDemoActive(true);
    const snapshot = await invoke<{
      configuration: { status: string };
      announcements: { data: { announcements: unknown[] } | null };
    }>("get_app_content_snapshot");
    expect(snapshot.configuration.status).toBe("ready");
    expect(snapshot.announcements.data?.announcements.length).toBeGreaterThan(0);
    expect(internalsInvoke).not.toHaveBeenCalled();
  });

  it("hands remote resource requests a decodable PNG instead of hitting IPC", async () => {
    setDemoActive(true);
    const bytes = await invoke<number[]>("load_remote_resource", {
      url: "https://example.invalid/a.png",
      kind: "announcement",
    });
    expect(internalsInvoke).not.toHaveBeenCalled();
    // PNG 签名，前端 imageMimeType 靠它选 MIME。
    expect(bytes.slice(0, 8)).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  });

  // 真实 Channel 只有 `onmessage`，没有 `send`——桩子必须照这个形状写，
  // 否则测不出「mock 根本没往页面推数据」这类问题。
  const sink = () => {
    const received: unknown[] = [];
    return {
      received,
      channel: { onmessage: (payload: unknown) => void received.push(payload) },
    };
  };

  it("pushes timetable and todo snapshots through a channel onmessage", async () => {
    setDemoActive(true);
    const timetable = sink();
    const todos = sink();

    await invoke("subscribe_timetable", { channel: timetable.channel });
    await invoke("subscribe_todos", { channel: todos.channel });

    expect(internalsInvoke).not.toHaveBeenCalled();
    expect(timetable.received).toEqual([
      expect.objectContaining({ type: "initial" }),
      expect.objectContaining({ type: "snapshot" }),
    ]);
    expect(todos.received).toHaveLength(2);
    const snapshot = (timetable.received[0] as { data: { sessions: unknown[] } }).data;
    expect(snapshot.sessions.length).toBeGreaterThan(0);
  });

  it("pushes OJ snapshot and lists over the channel", async () => {
    setDemoActive(true);
    const state = sink();
    const lists = sink();

    await invoke("subscribe_oj_state", { channel: state.channel });
    await invoke("subscribe_oj_lists", { channel: lists.channel });

    expect(internalsInvoke).not.toHaveBeenCalled();
    expect(state.received).toEqual([
      expect.objectContaining({ type: "initial" }),
      expect.objectContaining({ type: "snapshot" }),
    ]);
    const listPayload = lists.received[0] as {
      data: { groups: Record<string, unknown[]>; contests: Record<string, unknown[]> };
    };
    // 键口径：站点 ID 与 `site:group`。
    expect(Object.keys(listPayload.data.groups)).toEqual(["npuoj"]);
    expect(Object.keys(listPayload.data.contests)).toEqual(["npuoj:g-1"]);
  });

  it("serves an OJ paper with problems and a draft for the first one", async () => {
    setDemoActive(true);
    const paper = await invoke<{
      problems: { id: string; kind: number }[];
      drafts: Record<string, unknown>;
    }>("get_oj_paper", { siteId: "npuoj", contestId: "ct-1" });
    expect(internalsInvoke).not.toHaveBeenCalled();
    expect(paper.problems.length).toBeGreaterThan(0);
    expect(Object.keys(paper.drafts)).toEqual([
      `${paper.problems[0].kind}:${paper.problems[0].id}`,
    ]);
  });
});
