/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { OcrModelSettings, type OcrModelStatus } from "./OcrModelSettings";

vi.mock("./logical-navigation", () => ({
  useLogicalNavigation: () => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const initialStatus: OcrModelStatus = {
  status: "missing",
  revision: "c40f56f95412e10bcb9bd0bd24411e92f896d238",
  sha256: "2e68400494be9060b86a8ba699fce97cc649c01777d048ea6198f7e71c9fa0f4",
  bytes: 2294200,
};

const readyStatus: OcrModelStatus = {
  status: "ready",
  revision: "c40f56f95412e10bcb9bd0bd24411e92f896d238",
  sha256: "2e68400494be9060b86a8ba699fce97cc649c01777d048ea6198f7e71c9fa0f4",
  bytes: 2294200,
  source: "cache",
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
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") return initialStatus;
    if (command === "download_ocr_model") return readyStatus;
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders status and triggers OCR model download", async () => {
  render(<OcrModelSettings />);
  expect(await screen.findByText("尚未安装 OCR 模型")).toBeInTheDocument();

  const downloadBtn = screen.getByRole("button", { name: "下载模型" });
  fireEvent.click(downloadBtn);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("download_ocr_model"));
});

it("renders progress bar and real-time speed when OCR model is downloading", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") {
      return {
        status: "downloading",
        revision: "c40f56f95412e10bcb9bd0bd24411e92f896d238",
        sha256: "2e68400494be9060b86a8ba699fce97cc649c01777d048ea6198f7e71c9fa0f4",
        bytes: 2097152,
        downloadedBytes: 1572864,
        totalBytes: 2097152,
        speedBytesPerSec: 786432,
      };
    }
    return undefined;
  });

  render(<OcrModelSettings />);

  expect(await screen.findByText("75%")).toBeInTheDocument();
  expect(screen.getByText("768.0 KB/s")).toBeInTheDocument();
  expect(screen.getByText("1.5 MB / 2.0 MB")).toBeInTheDocument();
});
