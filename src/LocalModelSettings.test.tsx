/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "@heroui/react";
import {
  LocalModelCard,
  LocalModelsSettings,
  LOCAL_MODELS,
  type LocalModelStatus,
} from "./LocalModelSettings";

vi.mock("./logical-navigation", () => ({
  useLogicalNavigation: () => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const ocrManifest = LOCAL_MODELS.ocr.manifest;
const embeddingManifest = LOCAL_MODELS.embedding.manifest;

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
  Element.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders unified LocalModelsSettings displaying both OCR and Embedding models", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") {
      return {
        status: "ready",
        revision: ocrManifest.revision ?? "",
        sha256: ocrManifest.sha256,
        bytes: ocrManifest.bytes,
        source: "cache",
      } satisfies LocalModelStatus;
    }
    if (command === "get_embedding_model_status") {
      return {
        status: "missing",
        revision: embeddingManifest.revision ?? "",
        sha256: embeddingManifest.sha256,
        bytes: embeddingManifest.bytes,
      } satisfies LocalModelStatus;
    }
    return undefined;
  });

  render(<LocalModelsSettings />);

  // Heading and description
  expect(await screen.findByText("本地离线模型")).toBeInTheDocument();

  // Model categories
  expect(screen.getByText("OCR")).toBeInTheDocument();
  expect(screen.getByText("Embedding")).toBeInTheDocument();

  // Model names
  expect(screen.getByText("ddddocr")).toBeInTheDocument();
  expect(screen.getByText("BGE-small-zh-v1.5 (量化版)")).toBeInTheDocument();

  // Model application features
  expect(screen.getByText("选课系统与在线评测（OJ）验证码离线识别")).toBeInTheDocument();
  expect(screen.getByText("UOOC 题库检索与语义相似度计算")).toBeInTheDocument();

  // Model states
  expect(screen.getByText("模型已就绪")).toBeInTheDocument();
  expect(screen.getByText("尚未安装 Embedding 模型")).toBeInTheDocument();
});

it("checks update and reports already up to date when hash matches embedded model.json", async () => {
  const successSpy = vi.spyOn(toast, "success");

  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") {
      return {
        status: "ready",
        revision: ocrManifest.revision ?? "",
        sha256: ocrManifest.sha256, // matches embedded model.json exactly
        bytes: ocrManifest.bytes,
        source: "cache",
      } satisfies LocalModelStatus;
    }
    return undefined;
  });

  render(<LocalModelCard kind="ocr" />);

  expect(await screen.findByText("模型已就绪")).toBeInTheDocument();
  const checkBtn = screen.getByRole("button", { name: "检查更新" });
  fireEvent.click(checkBtn);

  await waitFor(() => {
    expect(successSpy).toHaveBeenCalledWith(
      expect.stringContaining("模型哈希与软件内嵌配置一致，已是最新版本"),
    );
    expect(invoke).not.toHaveBeenCalledWith("update_ocr_model");
  });
});

it("checks update and triggers update when hash does not match embedded model.json", async () => {
  const infoSpy = vi.spyOn(toast, "info");
  const successSpy = vi.spyOn(toast, "success");

  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_ocr_model_status") {
      return {
        status: "ready",
        revision: "old-revision",
        sha256: "0000000000000000000000000000000000000000000000000000000000000000", // different hash!
        bytes: ocrManifest.bytes,
        source: "cache",
      } satisfies LocalModelStatus;
    }
    if (command === "update_ocr_model") {
      return {
        status: "ready",
        revision: ocrManifest.revision ?? "",
        sha256: ocrManifest.sha256,
        bytes: ocrManifest.bytes,
        source: "cache",
      } satisfies LocalModelStatus;
    }
    return undefined;
  });

  render(<LocalModelCard kind="ocr" />);

  expect(await screen.findByText("模型已就绪")).toBeInTheDocument();
  const checkBtn = screen.getByRole("button", { name: "检查更新" });
  fireEvent.click(checkBtn);

  await waitFor(() => {
    expect(infoSpy).toHaveBeenCalledWith(expect.stringContaining("检测到模型配置已更新"));
    expect(invoke).toHaveBeenCalledWith("update_ocr_model");
    expect(successSpy).toHaveBeenCalledWith(expect.stringContaining("模型更新完成"));
  });
});

it("triggers force re-download when clicking 重新下载 / 修复", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_embedding_model_status") {
      return {
        status: "ready",
        revision: embeddingManifest.revision ?? "",
        sha256: embeddingManifest.sha256,
        bytes: embeddingManifest.bytes,
        source: "cache",
      } satisfies LocalModelStatus;
    }
    if (command === "update_embedding_model") {
      return {
        status: "ready",
        revision: embeddingManifest.revision ?? "",
        sha256: embeddingManifest.sha256,
        bytes: embeddingManifest.bytes,
        source: "cache",
      } satisfies LocalModelStatus;
    }
    return undefined;
  });

  render(<LocalModelCard kind="embedding" />);

  expect(await screen.findByText("模型已就绪")).toBeInTheDocument();
  const redownloadBtn = screen.getByRole("button", { name: "重新下载 / 修复" });
  fireEvent.click(redownloadBtn);

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("update_embedding_model");
  });
});
