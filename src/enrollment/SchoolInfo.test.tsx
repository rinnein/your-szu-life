/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { BatchInfo, OnlineUsers } from "./SchoolInfo";

type ChannelMock = { onmessage: ((message: unknown) => void) | null };
const channels: ChannelMock[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));
Element.prototype.getAnimations = () => [];

function captureOnlineChannels() {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "watch_enrollment_online_users") {
      const channel = (args as { channel?: ChannelMock } | undefined)?.channel;
      if (channel) channels.push(channel);
      return undefined;
    }
    return undefined;
  });
}

beforeEach(() => {
  channels.length = 0;
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});

it("displays school batch strategy and dates without inferring admission", async () => {
  vi.mocked(invoke).mockResolvedValue({
    dataList: [
      {
        code: "b",
        name: "新生复选",
        typeName: "复选阶段",
        tacticName: "先到先得",
        beginTime: "2026-10-07 16:00",
        endTime: "2026-10-08 16:00",
        active: "1",
      },
    ],
  });
  render(<BatchInfo account="batch-test" mode="campus" />);
  expect(await screen.findByText("策略：先到先得")).toBeVisible();
  expect(screen.getByText(/2026-10-07 16:00/)).toBeVisible();
  expect(screen.queryByText("学校未标记当前激活批次")).not.toBeInTheDocument();
  vi.mocked(invoke).mockRejectedValueOnce("网络超时");
  fireEvent.click(screen.getByRole("button", { name: "刷新批次" }));
  expect(await screen.findByText(/显示历史数据/)).toBeVisible();
  expect(screen.getByText("策略：先到先得")).toBeVisible();
});

it("ignores a late batch result from the previous access mode", async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke)
    .mockImplementationOnce(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    )
    .mockResolvedValue({ dataList: [{ name: "VPN批次", active: 1 }] });
  const view = render(<BatchInfo account="switch-test" mode="campus" />);
  view.rerender(<BatchInfo account="switch-test" mode="webvpn" />);
  await screen.findByText(/VPN批次/);
  await act(async () => resolve({ dataList: [{ name: "旧批次", active: 1 }] }));
  expect(screen.queryByText(/旧批次/)).not.toBeInTheDocument();
});

it("renders online users pushed by the backend channel", async () => {
  captureOnlineChannels();
  render(
    <>
      <OnlineUsers account="online-test" mode="campus" />
      <OnlineUsers account="online-test" mode="campus" />
    </>,
  );
  await waitFor(() => expect(channels).toHaveLength(2));
  await act(async () => {
    channels[0]?.onmessage?.({ online: 12, updatedAt: Date.now() });
    channels[1]?.onmessage?.({ online: 12, updatedAt: Date.now() });
  });
  expect(screen.getAllByText(/在线人数：12/)).toHaveLength(2);
});

it("shows unavailable online data without removing other controls", async () => {
  captureOnlineChannels();
  render(
    <>
      <OnlineUsers account="error-test" mode="campus" />
      <button>登录</button>
    </>,
  );
  await waitFor(() => expect(channels).toHaveLength(1));
  await act(async () => {
    channels[0]?.onmessage?.({ error: "超时" });
  });
  await waitFor(() => expect(screen.getByText(/更新失败/)).toBeVisible());
  expect(screen.getByRole("button", { name: "登录" })).toBeEnabled();
});
