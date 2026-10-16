import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ProgressBar } from "@heroui/react";

export type ModelDownloadProgress = {
  downloadedBytes: number;
  totalBytes: number;
  speedBytesPerSec: number;
};

export function formatDownloadSpeed(bytesPerSec: number): string {
  if (!bytesPerSec || bytesPerSec <= 0) return "0 B/s";
  if (bytesPerSec < 1024) return `${bytesPerSec} B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

export function formatModelBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function useModelDownloadProgress(kind: "ocr" | "embedding") {
  const eventName =
    kind === "ocr" ? "ocr-model-download-progress" : "embedding-model-download-progress";
  const [progress, setProgress] = useState<ModelDownloadProgress | null>(null);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<ModelDownloadProgress>(eventName, (event) => {
      if (active) {
        setProgress(event.payload);
      }
    }).then((off) => {
      if (!active) off();
      else stop = off;
    });

    return () => {
      active = false;
      stop?.();
    };
  }, [eventName]);

  return { progress, setProgress };
}

export function ModelProgressBar({
  downloadedBytes,
  totalBytes,
  speedBytesPerSec,
  label = "正在下载模型...",
}: {
  downloadedBytes: number;
  totalBytes: number;
  speedBytesPerSec: number;
  label?: string;
}) {
  const percent =
    totalBytes > 0
      ? Math.min(100, Math.max(0, Math.round((downloadedBytes / totalBytes) * 100)))
      : 0;

  return (
    <div className="grid w-full gap-2 rounded-xl border border-separator/40 bg-surface-secondary/40 p-3">
      <div className="flex items-center justify-between text-xs text-muted">
        <span>{label}</span>
        <span className="font-mono font-medium text-foreground">{percent}%</span>
      </div>
      <ProgressBar
        aria-label={label}
        value={percent}
        minValue={0}
        maxValue={100}
        className="w-full"
      >
        <ProgressBar.Track>
          <ProgressBar.Fill />
        </ProgressBar.Track>
      </ProgressBar>
      <div className="flex items-center justify-between font-mono text-xs text-muted">
        <span>
          {formatModelBytes(downloadedBytes)} / {formatModelBytes(totalBytes)}
        </span>
        <span className="font-medium text-foreground">
          {formatDownloadSpeed(speedBytesPerSec)}
        </span>
      </div>
    </div>
  );
}
