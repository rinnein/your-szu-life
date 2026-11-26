/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { QrScannerHost } from "./QrScannerHost";
import { isScanCancelled, runScanLoop, scanQrOnce, stopScanLoop } from "./index";

const fake = vi.hoisted(() => ({
  onDecode: null as ((result: { data: string }) => void) | null,
  start: vi.fn<() => Promise<void>>(),
  pause: vi.fn<() => Promise<boolean>>(),
  destroy: vi.fn(),
}));

vi.mock("qr-scanner", () => ({
  default: class {
    constructor(_video: HTMLVideoElement, onDecode: (result: { data: string }) => void) {
      fake.onDecode = onDecode;
    }

    start() {
      return fake.start();
    }

    pause() {
      return fake.pause();
    }

    destroy() {
      fake.destroy();
    }
  },
}));

vi.mock("../use-android", () => ({ useAndroid: () => "desktop" }));

function fakeStream(deviceId: string, label: string): MediaStream {
  const track = {
    label,
    stop: vi.fn(),
    getSettings: () => ({ deviceId }),
  };
  return {
    getVideoTracks: () => [track],
    getTracks: () => [track],
  } as unknown as MediaStream;
}

function dialog(): (HTMLElement & { close: () => void }) | null {
  return document.querySelector("m-dialog.qr-scan-dialog");
}

async function decode(data: string) {
  await waitFor(() => expect(fake.onDecode).not.toBeNull());
  act(() => fake.onDecode?.({ data }));
}

async function closeDialog() {
  await waitFor(() => expect(dialog()).not.toBeNull());
  act(() => dialog()?.close());
}

beforeEach(() => {
  Object.defineProperty(window.navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia: vi.fn(async () => fakeStream("cam-1", "后置摄像头")),
      enumerateDevices: vi.fn(async () => [
        { kind: "videoinput", deviceId: "cam-1", label: "后置摄像头" },
        { kind: "videoinput", deviceId: "cam-2", label: "前置摄像头" },
      ]),
    },
  });
  fake.onDecode = null;
  fake.start.mockReset();
  fake.start.mockResolvedValue(undefined);
  fake.pause.mockReset();
  fake.pause.mockResolvedValue(true);
  fake.destroy.mockReset();
});

afterEach(() => {
  stopScanLoop();
  cleanup();
  delete (window.navigator as { mediaDevices?: unknown }).mediaDevices;
});

describe("QrScannerHost", () => {
  it("resolves a one-shot scan with the decoded content", async () => {
    render(<QrScannerHost />);
    const pending = scanQrOnce();

    await decode("your-szu-life://page/pair-login?secret=abc");

    await expect(pending).resolves.toBe("your-szu-life://page/pair-login?secret=abc");
    await waitFor(() => expect(dialog()).toBeNull());
    expect(fake.destroy).toHaveBeenCalled();
  });

  it("exposes the dialog content part used by the fullscreen CSS", async () => {
    render(<QrScannerHost />);
    const pending = scanQrOnce();
    pending.catch(() => undefined);

    await waitFor(() => expect(dialog()).not.toBeNull());
    const content = dialog()?.shadowRoot?.querySelector('[part~="content"]');
    expect(content).not.toBeNull();

    await closeDialog();
    await pending.catch(() => undefined);
  });

  it("rejects with cancelled when the user closes the viewfinder", async () => {
    render(<QrScannerHost />);
    const pending = scanQrOnce();
    pending.catch(() => undefined);

    await closeDialog();

    const cause = await pending.catch((error: unknown) => error);
    expect(isScanCancelled(cause)).toBe(true);
  });

  it("keeps the dialog open and offers a retry when the camera fails", async () => {
    fake.start.mockRejectedValueOnce(new DOMException("denied", "NotAllowedError"));
    render(<QrScannerHost />);
    const pending = scanQrOnce();
    pending.catch(() => undefined);

    expect(await screen.findByText(/相机权限被拒绝/)).toBeInTheDocument();
    expect(dialog()).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(fake.start).toHaveBeenCalledTimes(2));

    await closeDialog();
    const cause = await pending.catch((error: unknown) => error);
    expect(isScanCancelled(cause)).toBe(true);
  });

  it("switches to the next camera when several are available", async () => {
    const media = window.navigator.mediaDevices;
    vi.mocked(media.getUserMedia)
      .mockResolvedValueOnce(fakeStream("cam-1", "后置摄像头"))
      .mockResolvedValueOnce(fakeStream("cam-2", "前置摄像头"));
    render(<QrScannerHost />);
    const pending = scanQrOnce();
    pending.catch(() => undefined);

    fireEvent.click(await screen.findByRole("button", { name: "切换摄像头" }));

    await waitFor(() => expect(fake.pause).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(media.getUserMedia).toHaveBeenCalledTimes(2));
    expect(vi.mocked(media.getUserMedia).mock.calls[1]?.[0]).toMatchObject({
      video: { deviceId: { exact: "cam-2" } },
    });
    await waitFor(() => expect(fake.start).toHaveBeenCalledTimes(2));

    await closeDialog();
    await pending.catch(() => undefined);
  });

  it("streams new codes in continuous mode and dedupes identical ones", async () => {
    render(<QrScannerHost />);
    const codes: string[] = [];
    const closed = vi.fn();
    const running = runScanLoop(
      (content) => codes.push(content),
      () => true,
      closed,
    );

    await decode("code-a");
    await decode("code-a");
    await decode("code-b");
    expect(codes).toEqual(["code-a", "code-b"]);

    await closeDialog();
    await running;
    expect(closed).toHaveBeenCalledTimes(1);
  });
});
