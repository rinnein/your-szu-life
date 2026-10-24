import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

export {
  DownloadSession,
  ModelProgressBar,
  formatBytes,
  formatDownloadSpeed,
  formatModelBytes,
  parseAria2Bitfield,
} from "./DownloadSession";
export type { DownloadSessionData, DownloadSessionProps } from "./DownloadSession";

export type ModelDownloadProgress = {
  downloadedBytes: number;
  totalBytes: number;
  speedBytesPerSec: number;
  port?: number;
  numPieces?: number;
  pieceLength?: number;
  bitfield?: string | null;
  connections?: number;
};

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
