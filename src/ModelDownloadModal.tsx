import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Chip, Modal } from "@heroui/react";
import { LuCircleCheck, LuCpu, LuDownload, LuSettings } from "react-icons/lu";
import { useLogicalNavigation } from "./logical-navigation";
import { formatModelSize } from "./LocalModelSettings";
import { useOcrModelStatus, type OcrModelStatus } from "./OcrModelSettings";
import { useEmbeddingModelStatus, type EmbeddingModelStatus } from "./EmbeddingModelSettings";
import { DownloadSession, useModelDownloadProgress } from "./ModelDownloadProgress";
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

  const { progress } = useModelDownloadProgress(kind);

  const [downloading, setDownloading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string>();
  const [navigating, setNavigating] = useState(false);

  const isReady = status?.status === "ready";
  const isDownloading =
    downloading || status?.status === "downloading" || (progress !== null && !isReady);

  const downloadedBytes = progress?.downloadedBytes ?? status?.downloadedBytes ?? 0;
  const totalBytes =
    progress?.totalBytes ?? status?.totalBytes ?? (status?.bytes || info.approxBytes);
  const speedBytesPerSec = progress?.speedBytesPerSec ?? status?.speedBytesPerSec ?? 0;

  const startDownload = async () => {
    setDownloading(true);
    setErrorMessage(undefined);
    try {
      await invoke(info.downloadCommand);
      onSuccess?.();
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setDownloading(false);
    }
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

  return (
    <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange}>
      <Modal.Container size="sm">
        <Modal.Dialog aria-labelledby="model-download-modal-heading">
          <Modal.CloseTrigger aria-label="关闭" />
          <Modal.Header>
            <Modal.Icon className="bg-primary-soft text-primary">
              <LuCpu className="size-5" />
            </Modal.Icon>
            <Modal.Heading id="model-download-modal-heading">{info.title}</Modal.Heading>
          </Modal.Header>
          <Modal.Body className="grid gap-3 text-sm">
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
                session={{
                  downloadedBytes,
                  totalBytes,
                  speedBytesPerSec,
                  port: progress?.port,
                  numPieces: progress?.numPieces,
                  pieceLength: progress?.pieceLength,
                  bitfield: progress?.bitfield,
                  connections: progress?.connections,
                }}
              />
            )}

            {isReady && !isDownloading && (
              <Alert status="success">
                <Alert.Content>
                  <Alert.Title className="flex items-center gap-2">
                    <LuCircleCheck aria-hidden="true" /> 模型已就绪
                  </Alert.Title>
                  <Alert.Description>模型已成功安装并校验通过，可离线使用。</Alert.Description>
                </Alert.Content>
              </Alert>
            )}

            {(errorMessage || status?.status === "error") && (
              <Alert status="danger">
                <Alert.Content>
                  <Alert.Title>下载遇到问题</Alert.Title>
                  <Alert.Description>
                    {errorMessage ?? status?.error ?? "下载或校验失败，请检查网络连接后重试"}
                  </Alert.Description>
                </Alert.Content>
              </Alert>
            )}
          </Modal.Body>
          <Modal.Footer className="flex flex-wrap items-center justify-between gap-2">
            <Button
              variant="tertiary"
              size="sm"
              isPending={navigating}
              isDisabled={isDownloading}
              onPress={() => void goToSettings()}
            >
              <LuSettings aria-hidden="true" /> 前往设置
            </Button>

            <div className="flex items-center gap-2">
              <Button
                variant="secondary"
                size="sm"
                isDisabled={isDownloading}
                onPress={() => onOpenChange(false)}
              >
                {isReady ? "关闭" : "稍后再说"}
              </Button>
              {!isReady && (
                <Button
                  size="sm"
                  isPending={isDownloading}
                  isDisabled={isDownloading}
                  onPress={() => void startDownload()}
                >
                  <LuDownload aria-hidden="true" /> 立即下载
                </Button>
              )}
            </div>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
