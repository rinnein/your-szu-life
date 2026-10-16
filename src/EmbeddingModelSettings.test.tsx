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
  revision: "ba24e2e75e6eb1a25c19f972c3d97f5a8c2b7f3b",
  sha256: "81f18544e39ec2f8b668079fef3f94e1d53039d91f4b8cf6b41295ea9f5e317c",
  bytes: 48942200,
};

const readyStatus: EmbeddingModelStatus = {
  status: "ready",
  revision: "ba24e2e75e6eb1a25c19f972c3d97f5a8c2b7f3b",
  sha256: "81f18544e39ec2f8b668079fef3f94e1d53039d91f4b8cf6b41295ea9f5e317c",
  bytes: 48942200,
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
        revision: "ba24e2e75e6eb1a25c19f972c3d97f5a8c2b7f3b",
        sha256: "81f18544e39ec2f8b668079fef3f94e1d53039d91f4b8cf6b41295ea9f5e317c",
        bytes: 48942200,
        downloadedBytes: 24471100,
        totalBytes: 48942200,
        speedBytesPerSec: 1048576,
      };
    }
    return undefined;
  });

  render(<EmbeddingModelSettings />);

  expect(await screen.findByText("50%")).toBeInTheDocument();
  expect(screen.getByText("1.0 MB/s")).toBeInTheDocument();
  expect(screen.getByText("23.3 MB / 46.7 MB")).toBeInTheDocument();
});
