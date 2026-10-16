/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { UoocQaSettings, type EmbeddingVectorStatus } from "./UoocQaSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const initialStatus: EmbeddingVectorStatus = {
  status: "empty",
  cachedCount: 0,
  totalDimension: 1536,
  updatedAtMs: 1000,
  message: "尚未下载题库向量",
};

const downloadedStatus: EmbeddingVectorStatus = {
  status: "ready",
  cachedCount: 88,
  totalDimension: 1536,
  updatedAtMs: 2000,
  message: "本地已缓存 88 条题目向量",
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
    if (command === "get_uooc_embedding_status") return initialStatus;
    if (command === "download_uooc_qa_vectors") return downloadedStatus;
    if (command === "sync_uooc_qa_bank") return 5;
    if (command === "search_uooc_qa_questions")
      return [
        {
          questionId: "q101",
          tid: "201",
          cid: "301",
          stem: "关于定积分中值定理，以下表述正确的是？",
          questionType: "single",
          similarity: 0.965,
          answers: ["B"],
          source: "本地题库向量检索",
        },
      ];
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders initial status and downloads uooc qa vectors", async () => {
  render(<UoocQaSettings />);
  expect(await screen.findByText("尚未下载题库向量")).toBeInTheDocument();

  const downloadBtn = screen.getByRole("button", { name: "下载 UOOC 标记题库向量" });
  fireEvent.click(downloadBtn);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("download_uooc_qa_vectors"));
  expect(await screen.findByText("题库向量库已就绪")).toBeInTheDocument();
  expect(screen.getAllByText(/88 条题目向量/).length).toBeGreaterThan(0);
});

it("syncs confirmed questions with backend", async () => {
  render(<UoocQaSettings />);
  const syncBtn = await screen.findByRole("button", { name: "同步已确认题目至服务端" });
  fireEvent.click(syncBtn);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("sync_uooc_qa_bank"));
  expect(await screen.findByText(/已成功与后端比对并将 5 道缺失题目同步上传/)).toBeInTheDocument();
});

it("searches questions using local embedding calculation", async () => {
  render(<UoocQaSettings />);
  const input = await screen.findByPlaceholderText(/输入题干文本或关键词/);
  fireEvent.change(input, { target: { value: "积分中值定理" } });

  const searchBtn = screen.getByRole("button", { name: "向量搜索" });
  expect(searchBtn).toBeEnabled();
  fireEvent.click(searchBtn);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("search_uooc_qa_questions", {
      query: "积分中值定理",
    }),
  );

  expect(await screen.findByText("关于定积分中值定理，以下表述正确的是？")).toBeInTheDocument();
  expect(screen.getByText("相似度 96.5%")).toBeInTheDocument();
  expect(screen.getByText("单选题")).toBeInTheDocument();
  expect(screen.getByText(/标准答案：/)).toBeInTheDocument();
});
