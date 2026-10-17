/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { EmbeddingModelSettings, type EmbeddingModelStatus } from "./EmbeddingModelSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const initialStatus: EmbeddingModelStatus = {
  status: "missing",
  revision: "75c43b069aac4d136ba6bc1122f995fedcfd2781",
  sha256: "15b717c382bcb518ba457b93ea6850ede7f4f1cd8937454aa06972366cd19bcc",
  bytes: 24010842,
};

const readyStatus: EmbeddingModelStatus = {
  status: "ready",
  revision: "75c43b069aac4d136ba6bc1122f995fedcfd2781",
  sha256: "15b717c382bcb518ba457b93ea6850ede7f4f1cd8937454aa06972366cd19bcc",
  bytes: 24010842,
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
    if (command === "get_embedding_model_status") return initialStatus;
    if (command === "download_embedding_model") return readyStatus;
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders status and downloads embedding model", async () => {
  render(<EmbeddingModelSettings />);
  expect(await screen.findByText("尚未安装 Embedding 模型")).toBeInTheDocument();

  const downloadBtn = screen.getByRole("button", { name: "下载 Embedding 模型" });
  fireEvent.click(downloadBtn);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("download_embedding_model"));
});

it("renders progress bar and real-time speed when embedding model is downloading", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_embedding_model_status") {
      return {
        status: "downloading",
        revision: "75c43b069aac4d136ba6bc1122f995fedcfd2781",
        sha256: "15b717c382bcb518ba457b93ea6850ede7f4f1cd8937454aa06972366cd19bcc",
        bytes: 24010842,
        downloadedBytes: 12005421,
        totalBytes: 24010842,
        speedBytesPerSec: 1048576,
      };
    }
    return undefined;
  });

  render(<EmbeddingModelSettings />);

  expect(await screen.findByText("50%")).toBeInTheDocument();
  expect(screen.getByText("1.0 MB/s")).toBeInTheDocument();
  expect(screen.getByText("11.4 MB / 22.9 MB")).toBeInTheDocument();
});
