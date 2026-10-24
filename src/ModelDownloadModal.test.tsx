/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { ModelDownloadModal } from "./ModelDownloadModal";
import type { ModelDownloadProgress } from "./ModelDownloadProgress";
import ocrModelManifest from "../src-tauri/resources/ocr/ddddocr/model.json";
import embeddingModelManifest from "../src-tauri/resources/embedding/bge-small-zh-v1.5/model.json";

const { nav } = vi.hoisted(() => ({
  nav: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
}));
vi.mock("./logical-navigation", () => ({ useLogicalNavigation: () => nav }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

let ocrProgressListener: EventCallback<ModelDownloadProgress> | undefined;
let embeddingProgressListener: EventCallback<ModelDownloadProgress> | undefined;

beforeEach(() => {
  vi.clearAllMocks();
  ocrProgressListener = undefined;
  embeddingProgressListener = undefined;

  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.getAnimations = () => [];

  vi.mocked(listen).mockImplementation(async (event, callback) => {
    if (event === "ocr-model-download-progress") {
      ocrProgressListener = callback as EventCallback<ModelDownloadProgress>;
    } else if (event === "embedding-model-download-progress") {
      embeddingProgressListener = callback as EventCallback<ModelDownloadProgress>;
    }
    return () => {};
  });

  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") {
      return {
        status: "missing",
        revision: ocrModelManifest.revision,
        sha256: ocrModelManifest.sha256,
        bytes: ocrModelManifest.bytes,
      };
    }
    if (command === "get_embedding_model_status") {
      return {
        status: "missing",
        revision: embeddingModelManifest.revision,
        sha256: embeddingModelManifest.sha256,
        bytes: embeddingModelManifest.bytes,
      };
    }
    if (command === "download_ocr_model" || command === "download_embedding_model") {
      return { status: "ready" };
    }
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders OCR model guidance modal and downloads on demand", async () => {
  const onOpenChange = vi.fn();
  const onSuccess = vi.fn();

  render(
    <ModelDownloadModal
      kind="ocr"
      isOpen={true}
      onOpenChange={onOpenChange}
      onSuccess={onSuccess}
    />,
  );

  expect(await screen.findByText("需要安装本地 OCR 模型")).toBeInTheDocument();
  expect(screen.getByText(/选课系统与在线评测（OJ）验证码/)).toBeInTheDocument();

  const downloadBtn = screen.getByRole("button", { name: /立即下载/ });
  fireEvent.click(downloadBtn);

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("download_ocr_model", {
      onEvent: expect.any(Object),
    });
    expect(onSuccess).toHaveBeenCalled();
  });
});

it("renders Embedding model guidance modal and navigates to settings", async () => {
  const onOpenChange = vi.fn();

  render(<ModelDownloadModal kind="embedding" isOpen={true} onOpenChange={onOpenChange} />);

  expect(await screen.findByText("需要安装本地 Embedding 模型")).toBeInTheDocument();
  expect(screen.getByText(/UOOC 题库检索与语义相似度计算/)).toBeInTheDocument();

  const settingsBtn = screen.getByRole("button", { name: /前往设置/ });
  fireEvent.click(settingsBtn);

  await waitFor(() => {
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(nav.push).toHaveBeenCalledWith("/settings?tab=models&focus=embedding");
  });
});

it("renders progress bar and real-time download speed when progress event fires", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") {
      return {
        status: "downloading",
        revision: "c40f56f95412e10bcb9bd0bd24411e92f896d238",
        sha256: "2e68400494be9060b86a8ba699fce97cc649c01777d048ea6198f7e71c9fa0f4",
        bytes: 2097152,
        downloadedBytes: 1048576,
        totalBytes: 2097152,
        speedBytesPerSec: 524288,
      };
    }
    return undefined;
  });

  render(<ModelDownloadModal kind="ocr" isOpen={true} onOpenChange={vi.fn()} />);

  expect(await screen.findByText("需要安装本地 OCR 模型")).toBeInTheDocument();
  expect(await screen.findByText("50%")).toBeInTheDocument();
  expect(screen.getByText("512.0 KB/s")).toBeInTheDocument();
  expect(screen.getByText("1.0 MB / 2.0 MB")).toBeInTheDocument();
});

it("updates real-time download progress and speed when live event fires", async () => {
  render(<ModelDownloadModal kind="embedding" isOpen={true} onOpenChange={vi.fn()} />);

  expect(await screen.findByText("需要安装本地 Embedding 模型")).toBeInTheDocument();

  act(() => {
    embeddingProgressListener?.({
      event: "embedding-model-download-progress",
      id: 1,
      payload: {
        downloadedBytes: 6002710,
        totalBytes: 24010842,
        speedBytesPerSec: 1572864,
      },
    });
  });

  expect(await screen.findByText("25%")).toBeInTheDocument();
  expect(screen.getByText("1.5 MB/s")).toBeInTheDocument();

  act(() => {
    ocrProgressListener?.({
      event: "ocr-model-download-progress",
      id: 2,
      payload: {
        downloadedBytes: 1048576,
        totalBytes: 2097152,
        speedBytesPerSec: 1048576,
      },
    });
  });
});
