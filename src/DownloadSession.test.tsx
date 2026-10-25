/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vite-plus/test";
import {
  DownloadSession,
  formatBytes,
  formatDownloadSpeed,
  parseAria2Bitfield,
} from "./DownloadSession";

afterEach(() => {
  cleanup();
});

it("accurately parses aria2 hex bitfield into boolean array", () => {
  // Empty inputs
  expect(parseAria2Bitfield(null, 0)).toEqual([]);
  expect(parseAria2Bitfield(undefined, 8)).toEqual([]);
  expect(parseAria2Bitfield("", 8)).toEqual([]);

  // "f0" with 8 pieces: 1111 0000 -> 4 true, 4 false
  expect(parseAria2Bitfield("f0", 8)).toEqual([true, true, true, true, false, false, false, false]);

  // "80" with 8 pieces: 1000 0000 -> 1 true, 7 false
  expect(parseAria2Bitfield("80", 8)).toEqual([
    true,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
  ]);

  // "ffc0" with 10 pieces: 1111 1111 1100 0000 -> 8 true, 2 true = 10 true (trailing padding discarded)
  expect(parseAria2Bitfield("ffc0", 10)).toEqual([
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
  ]);

  // "a0" with 6 pieces: 1010 0000 -> [true, false, true, false, false, false]
  expect(parseAria2Bitfield("a0", 6)).toEqual([true, false, true, false, false, false]);
});

it("formats speed and byte metrics properly", () => {
  expect(formatDownloadSpeed(0)).toBe("0 B/s");
  expect(formatDownloadSpeed(1048576)).toBe("1.0 MB/s");
  expect(formatBytes(0)).toBe("0 B");
  expect(formatBytes(52428800)).toBe("50.0 MB");
});

it("renders session card with port, connections, speed and progress", () => {
  render(
    <DownloadSession
      title="正在下载并校验 OCR 模型..."
      session={{
        downloadedBytes: 25000000,
        totalBytes: 50000000,
        speedBytesPerSec: 3145728,
        port: 54001,
        connections: 16,
        numPieces: 50,
        pieceLength: 1048576,
        bitfield: "ffff0000000000",
      }}
    />,
  );

  expect(screen.getByText("正在下载并校验 OCR 模型...")).toBeInTheDocument();
  expect(screen.getByText("RPC :54001")).toBeInTheDocument();
  expect(screen.getByText("16 线程")).toBeInTheDocument();
  expect(screen.getByText("50%")).toBeInTheDocument();
  expect(screen.getByText("3.0 MB/s")).toBeInTheDocument();
  expect(screen.getByText("分片数: 50")).toBeInTheDocument();
});

it("toggles expandable piece details and renders piece block matrix", () => {
  render(
    <DownloadSession
      title="下载任务"
      session={{
        downloadedBytes: 1048576,
        totalBytes: 2097152,
        speedBytesPerSec: 1048576,
        numPieces: 4,
        pieceLength: 524288,
        bitfield: "c0", // 1100 0000 -> 2 pieces done, 2 pending
      }}
    />,
  );

  const toggleBtn = screen.getByRole("button", { name: /分片进度/ });
  expect(toggleBtn).toBeInTheDocument();
  expect(screen.getByText(/2 \/ 4 完成/)).toBeInTheDocument();

  // Initially collapsed
  expect(screen.queryByRole("region", { name: "分片下载状态矩阵" })).not.toBeInTheDocument();

  // Click to expand
  fireEvent.click(toggleBtn);
  expect(screen.getByRole("region", { name: "分片下载状态矩阵" })).toBeInTheDocument();
  expect(screen.getByText("50.0%")).toBeInTheDocument();

  // Click to collapse
  fireEvent.click(toggleBtn);
  expect(screen.queryByRole("region", { name: "分片下载状态矩阵" })).not.toBeInTheDocument();
});

it("shows piece block matrix immediately when defaultExpanded is set", () => {
  render(
    <DownloadSession
      title="下载任务"
      defaultExpanded
      session={{
        downloadedBytes: 1048576,
        totalBytes: 2097152,
        speedBytesPerSec: 1048576,
        numPieces: 4,
        pieceLength: 524288,
        bitfield: "c0",
      }}
    />,
  );

  // 分片矩阵无需交互即可见，下载开始就能看到分片进度
  expect(screen.getByRole("region", { name: "分片下载状态矩阵" })).toBeInTheDocument();
  expect(screen.getByText(/2 \/ 4 完成/)).toBeInTheDocument();

  // 仍可折叠回去
  fireEvent.click(screen.getByRole("button", { name: /分片进度/ }));
  expect(screen.queryByRole("region", { name: "分片下载状态矩阵" })).not.toBeInTheDocument();
});

it("hides the piece panel entirely when aria2 reports no pieces", () => {
  render(
    <DownloadSession
      title="下载任务"
      defaultExpanded
      session={{ downloadedBytes: 0, totalBytes: 1000 }}
    />,
  );

  // numPieces 缺失时不渲染分片区域，避免空矩阵
  expect(screen.queryByRole("button", { name: /分片进度/ })).not.toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "分片下载状态矩阵" })).not.toBeInTheDocument();
});

it("renders breakpoint resumed badge and total active tasks count", () => {
  render(
    <DownloadSession
      title="断点续传任务"
      session={{
        downloadedBytes: 30000000,
        totalBytes: 50000000,
        speedBytesPerSec: 2097152,
        port: 54001,
        totalActiveTasks: 2,
        isResumed: true,
        resumedBytes: 20000000,
        numPieces: 10,
        pieceLength: 5000000,
        bitfield: "f0",
      }}
    />,
  );

  expect(screen.getByText("断点续传已恢复")).toBeInTheDocument();
  expect(screen.getByText("2 个任务并发")).toBeInTheDocument();
  expect(screen.getByText(/已恢复 19.1 MB/)).toBeInTheDocument();
  expect(screen.getByText("RPC :54001")).toBeInTheDocument();
});

it("distinguishes started (light green) and not-yet-started (white) pieces", () => {
  const { rerender } = render(
    <DownloadSession
      title="下载任务"
      defaultExpanded
      session={{ downloadedBytes: 0, totalBytes: 2097152, numPieces: 4, bitfield: "00" }}
    />,
  );

  // 未产生任何下载进度：所有分片应显示为“未开始下载”
  let blocks = screen
    .getByRole("region", { name: "分片下载状态矩阵" })
    .querySelectorAll("[data-piece-state]");
  expect(blocks).toHaveLength(4);
  expect(Array.from(blocks).every((el) => el.getAttribute("data-piece-state") === "pending")).toBe(
    true,
  );

  rerender(
    <DownloadSession
      title="下载任务"
      defaultExpanded
      session={{ downloadedBytes: 1048576, totalBytes: 2097152, numPieces: 4, bitfield: "80" }}
    />,
  );

  // 已完成 → done，其余未完成但有进度 → downloading
  blocks = screen
    .getByRole("region", { name: "分片下载状态矩阵" })
    .querySelectorAll("[data-piece-state]");
  const states = Array.from(blocks).map((el) => el.getAttribute("data-piece-state"));
  expect(states).toEqual(["done", "downloading", "downloading", "downloading"]);

  // 图例同步显示三态
  expect(screen.getByText("未开始下载")).toBeInTheDocument();
  expect(screen.getByText("下载中")).toBeInTheDocument();
  expect(screen.getByText("已完成")).toBeInTheDocument();
});
