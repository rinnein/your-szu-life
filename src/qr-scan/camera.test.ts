/** @vitest-environment jsdom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { cameraErrorMessage, detectCamera } from "./camera";

const platform = vi.hoisted(() => ({ android: false }));

vi.mock("../os-info", () => ({
  isAndroidPlatform: () => Promise.resolve(platform.android),
}));

type Device = { kind: string };

function setMediaDevices(value: unknown) {
  Object.defineProperty(window.navigator, "mediaDevices", {
    configurable: true,
    value,
  });
}

function media(devices: Device[] | (() => Promise<Device[]>)) {
  return {
    getUserMedia: vi.fn(),
    enumerateDevices: vi.fn(
      typeof devices === "function" ? devices : () => Promise.resolve(devices),
    ),
  };
}

beforeEach(() => {
  platform.android = false;
});

afterEach(() => {
  delete (window.navigator as { mediaDevices?: unknown }).mediaDevices;
});

describe("detectCamera", () => {
  it("is unavailable without getUserMedia", async () => {
    setMediaDevices(undefined);
    await expect(detectCamera()).resolves.toBe(false);
  });

  it("always treats Android as having a camera", async () => {
    platform.android = true;
    setMediaDevices(media([]));
    await expect(detectCamera()).resolves.toBe(true);
  });

  it("detects a desktop video input", async () => {
    setMediaDevices(media([{ kind: "audioinput" }, { kind: "videoinput" }]));
    await expect(detectCamera()).resolves.toBe(true);
  });

  it("hides scanning when enumeration succeeds without a camera", async () => {
    setMediaDevices(media([{ kind: "audioinput" }]));
    await expect(detectCamera()).resolves.toBe(false);
  });

  it("stays optimistic when the device list is empty", async () => {
    setMediaDevices(media([]));
    await expect(detectCamera()).resolves.toBe(true);
  });

  it("stays optimistic when enumeration fails", async () => {
    setMediaDevices(media(() => Promise.reject(new Error("blocked"))));
    await expect(detectCamera()).resolves.toBe(true);
  });
});

describe("cameraErrorMessage", () => {
  it("explains a denied permission", () => {
    const failure = cameraErrorMessage(new DOMException("denied", "NotAllowedError"));
    expect(failure.permission).toBe(true);
    expect(failure.message).toContain("相机权限被拒绝");
  });

  it("explains a missing camera", () => {
    const failure = cameraErrorMessage(new DOMException("missing", "NotFoundError"));
    expect(failure.permission).toBe(false);
    expect(failure.message).toContain("未检测到可用的摄像头");
  });

  it("explains a camera busy error", () => {
    const failure = cameraErrorMessage(new DOMException("busy", "NotReadableError"));
    expect(failure.message).toContain("占用");
  });

  it("maps qr-scanner's camera-not-found message", () => {
    const failure = cameraErrorMessage(new Error("Camera not found"));
    expect(failure.message).toContain("未检测到可用的摄像头");
  });

  it("falls back to a generic retry message", () => {
    const failure = cameraErrorMessage(new Error("boom"));
    expect(failure.message).toContain("请重试");
  });
});
