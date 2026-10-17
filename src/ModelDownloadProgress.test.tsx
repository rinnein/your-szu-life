/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { expect, it } from "vite-plus/test";
import { formatDownloadSpeed, formatModelBytes, ModelProgressBar } from "./ModelDownloadProgress";

it("formats speed values accurately", () => {
  expect(formatDownloadSpeed(0)).toBe("0 B/s");
  expect(formatDownloadSpeed(500)).toBe("500 B/s");
  expect(formatDownloadSpeed(1024)).toBe("1.0 KB/s");
  expect(formatDownloadSpeed(1536)).toBe("1.5 KB/s");
  expect(formatDownloadSpeed(1048576)).toBe("1.0 MB/s");
  expect(formatDownloadSpeed(2621440)).toBe("2.5 MB/s");
});

it("formats byte values accurately", () => {
  expect(formatModelBytes(0)).toBe("0 B");
  expect(formatModelBytes(500)).toBe("500 B");
  expect(formatModelBytes(1024)).toBe("1.0 KB");
  expect(formatModelBytes(1048576)).toBe("1.0 MB");
  expect(formatModelBytes(48942200)).toBe("46.7 MB");
});

it("renders ModelProgressBar with correct progress percentage and labels", () => {
  render(
    <ModelProgressBar
      downloadedBytes={24471100}
      totalBytes={48942200}
      speedBytesPerSec={2097152}
      label="正在下载 Embedding 模型..."
    />,
  );

  expect(screen.getByText("正在下载 Embedding 模型...")).toBeInTheDocument();
  expect(screen.getByText("50%")).toBeInTheDocument();
  expect(screen.getByText("2.0 MB/s")).toBeInTheDocument();
  expect(screen.getByText("23.3 MB / 46.7 MB")).toBeInTheDocument();
});
