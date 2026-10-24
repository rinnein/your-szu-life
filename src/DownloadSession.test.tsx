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
  expect(parseAria2Bitfield("f0", 8)).toEqual([
    true,
    true,
    true,
    true,
    false,
    false,
    false,
    false,
  ]);

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
