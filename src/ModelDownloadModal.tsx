import { useState } from "react";
import { errorMessage as errorMessageText } from "./async-state";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Dialog, Button } from "material/react";
import { Banner, Chip } from "./ui";
import { LuCircleCheck, LuDownload, LuSettings } from "react-icons/lu";
import { useLogicalNavigation } from "./logical-navigation";
import { formatModelSize } from "./LocalModelSettings";
import { useOcrModelStatus, type OcrModelStatus } from "./OcrModelSettings";
import { useEmbeddingModelStatus, type EmbeddingModelStatus } from "./EmbeddingModelSettings";
import {
  DownloadSession,
  useModelDownloadProgress,
  type ModelDownloadChannelEvent,
} from "./ModelDownloadProgress";
import { createTaskChannel } from "./tauri-channel";
import ocrModelManifest from "../src-tauri/resources/ocr/ddddocr/model.json";
import embeddingModelManifest from "../src-tauri/resources/embedding/bge-small-zh-v1.5/model.json";

export type ModelKind = "ocr" | "embedding";

export const MODEL_INFO: Record<
  ModelKind,
  {
    title: string;
    description: string;
    sizeHint: string;
    approxBytes: number;
    downloadCommand: string;
    settingsFocus: string;
  }
> = {
  ocr: {
    title: ocrModelManifest.title,
    description: ocrModelManifest.description,
    sizeHint: `约 ${formatModelSize(ocrModelManifest.bytes)}`,
    approxBytes: ocrModelManifest.bytes,
    downloadCommand: ocrModelManifest.downloadCommand,
    settingsFocus: ocrModelManifest.settingsFocus,
  },
  embedding: {
    title: embeddingModelManifest.title,
    description: embeddingModelManifest.description,
    sizeHint: `约 ${formatModelSize(embeddingModelManifest.bytes)}`,
    approxBytes: embeddingModelManifest.bytes,
    downloadCommand: embeddingModelManifest.downloadCommand,
    settingsFocus: embeddingModelManifest.settingsFocus,
  },
};

export function ModelDownloadModal({
  kind,
  isOpen,
  onOpenChange,
  onSuccess,
}: {
  kind: ModelKind;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess?: () => void;
}) {
  const navigation = useLogicalNavigation();
  const info = MODEL_INFO[kind];

  const ocrStatus = useOcrModelStatus();
  const embeddingStatus = useEmbeddingModelStatus();
  const status: OcrModelStatus | EmbeddingModelStatus | undefined =
    kind === "ocr" ? ocrStatus : embeddingStatus;

  const { progress, setProgress } = useModelDownloadProgress(kind);

  /**
   * 下载过程的单一状态：idle → downloading → ready / failed。
   * 失败时清掉 progress，避免「有进度残留 + 未就绪」把派生的 isDownloading 锁死在 true。
   */
  type Phase = "idle" | "downloading" | "failed";
  const [phase, setPhase] = useState<Phase>("idle");
  const [errorMessage, setErrorMessage] = useState<string>();
  const [navigating, setNavigating] = useState(false);

  const isReady = status?.status === "ready";
  // 本组件发起（phase）、后端已有下载会话（status）或已有活跃进度（progress 未就绪）
  // 都视作下载中；失败会清 progress 并显式进入 failed，因此不会再被进度残留锁死。
  const isDownloading =
    phase === "downloading" ||
    status?.status === "downloading" ||
    (progress !== null && !isReady && phase !== "failed");

  const downloadedBytes = progress?.downloadedBytes ?? status?.downloadedBytes ?? 0;
  const totalBytes =
    progress?.totalBytes ?? status?.totalBytes ?? (status?.bytes || info.approxBytes);
  const speedBytesPerSec = progress?.speedBytesPerSec ?? status?.speedBytesPerSec ?? 0;

  const startDownload = async () => {
    setPhase("downloading");
    setErrorMessage(undefined);
    try {
      const channel = createTaskChannel<ModelDownloadChannelEvent>((event) => {
        if (event.type === "progress") {
          setProgress(event.data);
        }
      });
      await invoke(info.downloadCommand, { onEvent: channel });
      onSuccess?.();
    } catch (error) {
      setErrorMessage(errorMessageText(error, "下载或校验失败，请检查网络连接后重试"));
      setProgress(null);
      setPhase("failed");
      return;
    }
    setPhase("idle");
  };

  const goToSettings = async () => {
    setNavigating(true);
    try {
      onOpenChange(false);
      await navigation.push(`/settings?tab=models&focus=${info.settingsFocus}`);
    } finally {
      setNavigating(false);
    }
  };

  if (!isOpen || typeof document === "undefined") return null;

  return createPortal(
    <Dialog open={isOpen} headline={info.title} onClose={() => onOpenChange(false)}>
      <div className="grid gap-3 text-sm">
        <p className="leading-relaxed text-muted">{info.description}</p>

        <div className="flex items-center gap-2">
          <Chip size="sm" variant="soft">
            模型体积：{info.sizeHint}
          </Chip>
          <Chip size="sm" variant="soft">
            纯离线运行
          </Chip>
        </div>

        {isDownloading && (
          <DownloadSession
            title={`正在下载并校验 ${info.title}...`}
            defaultExpanded
            session={{
              downloadedBytes,
              totalBytes,
              speedBytesPerSec,
              port: progress?.port,
              numPieces: progress?.numPieces,
              pieceLength: progress?.pieceLength,
              bitfield: progress?.bitfield,
              connections: progress?.connections,
              isResumed: progress?.isResumed,
              resumedBytes: progress?.resumedBytes,
              totalActiveTasks: progress?.totalActiveTasks,
            }}
          />
        )}

        {isReady && !isDownloading && (
          <Banner status="success">
            <Banner.Title className="flex items-center gap-2">
              <LuCircleCheck aria-hidden="true" /> 模型已就绪
            </Banner.Title>
            <Banner.Description>模型已成功安装并校验通过，可离线使用。</Banner.Description>
          </Banner>
        )}

        {phase === "failed" && (
          <Banner status="danger">
            <Banner.Title>下载遇到问题</Banner.Title>
            <Banner.Description>
              {errorMessage ?? status?.error ?? "下载或校验失败，请检查网络连接后重试"}
            </Banner.Description>
          </Banner>
        )}
      </div>

      <div slot="actions" className="flex flex-wrap items-center justify-between gap-2 w-full">
        <Button
          variant="text"
          size="xs"
          disabled={isDownloading || navigating}
          onClick={() => void goToSettings()}
        >
          <LuSettings aria-hidden="true" /> 前往设置
        </Button>

        <div className="flex items-center gap-2">
          <Button
            variant="tonal"
            size="xs"
            disabled={isDownloading}
            onClick={() => onOpenChange(false)}
          >
            {isReady ? "关闭" : "稍后再说"}
          </Button>
          {!isReady && (
            <Button
              size="xs"
              disabled={isDownloading}
              onClick={() => void startDownload()}
            >
              <LuDownload aria-hidden="true" /> 立即下载
            </Button>
          )}
        </div>
      </div>
    </Dialog>,
    document.body,
  );
}
