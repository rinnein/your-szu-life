/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { RemoteImage, imageMimeType } from "./remote";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const pngBytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0])
  .buffer;

let createObjectURL: ReturnType<typeof vi.fn>;

beforeEach(() => {
  createObjectURL = vi.fn(() => "blob:remote-image");
  Object.defineProperty(URL, "createObjectURL", {
    value: createObjectURL,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    value: vi.fn(),
    configurable: true,
    writable: true,
  });
  vi.mocked(invoke).mockResolvedValue(pngBytes);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("imageMimeType", () => {
  it("covers the formats allowed by the question-bank contract", () => {
    const head = (...bytes: number[]) =>
      new Uint8Array([...bytes, ...Array.from({ length: 12 - bytes.length }, () => 0)]).buffer;
    expect(imageMimeType(head(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a))).toBe("image/png");
    expect(imageMimeType(head(0xff, 0xd8, 0xff))).toBe("image/jpeg");
    expect(imageMimeType(head(0x47, 0x49, 0x46, 0x38))).toBe("image/gif");
    expect(imageMimeType(head(0x42, 0x4d))).toBe("image/bmp");
    expect(imageMimeType(head(0x52, 0x49, 0x46, 0x46, 0, 0, 0, 0, 0x57, 0x45, 0x42, 0x50))).toBe(
      "image/webp",
    );
    expect(imageMimeType(head(0, 0, 0, 0, 0x66, 0x74, 0x79, 0x70, 0x61, 0x76, 0x69, 0x66))).toBe(
      "image/avif",
    );
    expect(imageMimeType(head(0x3c, 0x68, 0x74, 0x6d, 0x6c))).toBeUndefined();
  });
});

describe("RemoteImage", () => {
  it("renders data urls directly without invoking the native proxy", () => {
    render(<RemoteImage src="data:image/png;base64,AAAA" alt="内联图片" />);
    expect(screen.getByRole("img", { name: "内联图片" })).toHaveAttribute(
      "src",
      "data:image/png;base64,AAAA",
    );
    expect(invoke).not.toHaveBeenCalled();
  });

  it("loads http urls through the unified rust proxy", async () => {
    render(<RemoteImage src="https://a.com/x.png" alt="远程图片" />);
    await waitFor(() => expect(screen.queryByRole("img", { name: "远程图片" })).not.toBeNull());
    expect(screen.getByRole("img", { name: "远程图片" })).toHaveAttribute(
      "src",
      "blob:remote-image",
    );
    expect(invoke).toHaveBeenCalledWith("load_remote_resource", {
      url: "https://a.com/x.png",
      kind: "image",
    });
    expect(createObjectURL).toHaveBeenCalledTimes(1);
  });

  it("shows alt text when the proxy rejects the image", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("denied"));
    render(<RemoteImage src="https://a.com/private.png" alt="加载失败图片" />);
    expect(await screen.findByText("加载失败图片")).toBeInTheDocument();
  });

  it("accepts number-array payloads delivered by the android postMessage ipc", async () => {
    // Android 端 raw IPC 响应经 postMessage 回退时会序列化成 JSON number array，而不是 ArrayBuffer。
    vi.mocked(invoke).mockResolvedValueOnce(Array.from(new Uint8Array(pngBytes)));
    render(<RemoteImage src="https://a.com/bytes.png" alt="数组图片" />);
    await waitFor(() => expect(screen.queryByRole("img", { name: "数组图片" })).not.toBeNull());
    expect(screen.getByRole("img", { name: "数组图片" })).toHaveAttribute(
      "src",
      "blob:remote-image",
    );
    expect(createObjectURL).toHaveBeenCalledWith(expect.objectContaining({ type: "image/png" }));
  });
});
