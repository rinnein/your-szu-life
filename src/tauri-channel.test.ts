// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { createTaskChannel, useChannelSubscription } from "./tauri-channel";

vi.mock("@tauri-apps/api/core", () => {
  return {
    Channel: class MockChannel<T> {
      onmessage: ((message: T) => void) | null = null;
    },
    invoke: vi.fn().mockResolvedValue(undefined),
  };
});

describe("tauri-channel", () => {
  it("createTaskChannel sets up onmessage callback", () => {
    const callback = vi.fn();
    const channel = createTaskChannel(callback);
    expect(channel.onmessage).toBe(callback);

    const testPayload = { status: "downloading" };
    channel.onmessage?.(testPayload);
    expect(callback).toHaveBeenCalledWith(testPayload);
  });

  it("useChannelSubscription invokes command with channel parameter", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const onMessage = vi.fn();

    renderHook(() => useChannelSubscription("test_subscribe", onMessage, { extra: 123 }));

    expect(invoke).toHaveBeenCalledWith("test_subscribe", {
      extra: 123,
      channel: expect.any(Object),
    });
  });
});
