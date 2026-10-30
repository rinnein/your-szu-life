import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { Card, LoadingIndicator, Button } from "material/react";
import { Banner, Chip, toast } from "./ui";
import { LuCircleCheck, LuCpu, LuDownload, LuRefreshCw, LuTriangleAlert } from "react-icons/lu";
import { useLogicalNavigation } from "./logical-navigation";
import {
  DownloadSession,
  useModelDownloadProgress,
  type ModelDownloadChannelEvent,
} from "./ModelDownloadProgress";
import { createTaskChannel } from "./tauri-channel";
import { useRegisterNextAction, type StepNextAction } from "./step-next-action";
import ocrModelManifest from "../src-tauri/resources/ocr/ddddocr/model.json";
import embeddingModelManifest from "../src-tauri/resources/embedding/bge-small-zh-v1.5/model.json";

export type ModelKind = "ocr" | "embedding";

export type LocalModelStatus = {
  status: "ready" | "missing" | "downloading" | "error" | string;
  revision: string;
  sha256: string;
  bytes: number;
  source?: string;
  error?: string;
  downloadedBytes?: number;
  totalBytes?: number;
  speedBytesPerSec?: number;
};

export type OcrModelStatus = LocalModelStatus;
export type EmbeddingModelStatus = LocalModelStatus;

export function formatModelSize(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export interface ModelMetadata {
  kind: ModelKind;
  category: "OCR" | "Embedding";
  name: string;
  features: string;
  description: string;
  manifest: {
    sha256: string;
    bytes: number;
    revision?: string;
  };
  downloadCommand: string;
  updateCommand: string;
  statusCommand: string;
  statusEvent: string;
  anchorId: string;
  defaultDownloadLabel: string;
}

export const LOCAL_MODELS: Record<ModelKind, ModelMetadata> = {
  ocr: {
    kind: "ocr",
    category: "OCR",
    name: ocrModelManifest.name ?? "ddddocr",
    features: "选课系统与在线评测（OJ）验证码离线识别",
    description:
      ocrModelManifest.description ??
      "用于选课系统与在线评测（OJ）验证码离线自动识别。模型仅保存在本机应用数据目录，保护账号隐私安全。",
    manifest: ocrModelManifest,
    downloadCommand: ocrModelManifest.downloadCommand ?? "download_ocr_model",
    updateCommand: "update_ocr_model",
    statusCommand: "get_ocr_model_status",
    statusEvent: "ocr-model-status-changed",
    anchorId: "local-model-ocr",
    defaultDownloadLabel: "下载模型",
  },
  embedding: {
    kind: "embedding",
    category: "Embedding",
    name: embeddingModelManifest.name ?? "BGE-small-zh-v1.5 (量化版)",
    features: "UOOC 题库检索与语义相似度计算",
    description:
      embeddingModelManifest.description ??
      "用于 UOOC 题库检索与语义相似度计算，支持离线高速向量特征计算。",
    manifest: embeddingModelManifest,
    downloadCommand: embeddingModelManifest.downloadCommand ?? "download_embedding_model",
    updateCommand: "update_embedding_model",
    statusCommand: "get_embedding_model_status",
    statusEvent: "embedding-model-status-changed",
    anchorId: "local-model-embedding",
    defaultDownloadLabel: "下载 Embedding 模型",
  },
};

export function useLocalModelStatus(kind: ModelKind) {
  const [status, setStatus] = useState<LocalModelStatus>();
  const meta = LOCAL_MODELS[kind];

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<LocalModelStatus>(meta.statusEvent, (event) => {
      if (active) setStatus(event.payload);
    }).then((off) => {
      if (!active) off();
      else stop = off;
    });
    void invoke<LocalModelStatus>(meta.statusCommand)
      .then((value) => active && setStatus(value))
      .catch(
        () =>
          active &&
          setStatus({
            status: "ready",
            revision: meta.manifest.revision ?? "",
            sha256: meta.manifest.sha256,
            bytes: meta.manifest.bytes,
            source: "web-preview",
          }),
      );
    return () => {
      active = false;
      stop?.();
    };
  }, [meta]);

  return status;
}

export function useOcrModelStatus() {
  return useLocalModelStatus("ocr");
}

export function useEmbeddingModelStatus() {
  return useLocalModelStatus("embedding");
}

function useOptionalLogicalNavigation() {
  try {
    return useLogicalNavigation();
  } catch {
    return undefined;
  }
}

export interface LocalModelCardProps {
  kind: ModelKind;
  autoReturn?: boolean;
  downloadButtonLabel?: string;
  className?: string;
}

export function LocalModelCard({
  kind,
  autoReturn = false,
  downloadButtonLabel,
  className,
}: LocalModelCardProps) {
  const navigation = useOptionalLogicalNavigation();
  const meta = LOCAL_MODELS[kind];
  const status = useLocalModelStatus(kind);
  const { progress, setProgress } = useModelDownloadProgress(kind);

  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>();

  const ready = status?.status === "ready";
  const isDownloading = busy || status?.status === "downloading" || (progress !== null && !ready);

  const downloadedBytes = progress?.downloadedBytes ?? status?.downloadedBytes ?? 0;
  const totalBytes =
    progress?.totalBytes ?? status?.totalBytes ?? status?.bytes ?? meta.manifest.bytes;
  const speedBytesPerSec = progress?.speedBytesPerSec ?? status?.speedBytesPerSec ?? 0;

  const handleDownload = async (isUpdate: boolean) => {
    setBusy(true);
    setMessage(undefined);
    try {
      const channel = createTaskChannel<ModelDownloadChannelEvent>((event) => {
        if (event.type === "progress") {
          setProgress(event.data);
        }
      });
      await invoke<LocalModelStatus>(isUpdate ? meta.updateCommand : meta.downloadCommand, {
        onEvent: channel,
      });
      toast.success(isUpdate ? `${meta.name} 重新下载完成` : `${meta.name} 下载完成`);
      if (autoReturn && navigation) await navigation.back();
    } catch (error) {
      const errorMsg = String(error);
      setMessage(errorMsg);
      toast.danger(`${isUpdate ? "更新" : "下载"}失败`, { description: errorMsg });
    } finally {
      setBusy(false);
    }
  };

  const handleCheckUpdate = async () => {
    setBusy(true);
    setMessage(undefined);
    try {
      let currentStatus: LocalModelStatus | undefined = status;
      try {
        currentStatus = await invoke<LocalModelStatus>(meta.statusCommand);
      } catch {
        // Fall back to status from hook
      }

      if (currentStatus?.status !== "ready") {
        toast.warning(`${meta.name} 尚未安装，请点击下载模型`);
        return;
      }

      const installedSha = (currentStatus?.sha256 ?? "").toLowerCase().trim();
      const expectedSha = meta.manifest.sha256.toLowerCase().trim();

      if (installedSha && installedSha !== expectedSha) {
        toast.info(`检测到模型配置已更新（内嵌哈希不匹配），正在开始下载更新...`);
        const channel = createTaskChannel<ModelDownloadChannelEvent>((event) => {
          if (event.type === "progress") {
            setProgress(event.data);
          }
        });
        await invoke<LocalModelStatus>(meta.updateCommand, { onEvent: channel });
        toast.success(`${meta.name} 模型更新完成`);
        if (autoReturn && navigation) await navigation.back();
      } else {
        toast.success(`${meta.name} 模型哈希与软件内嵌配置一致，已是最新版本`);
      }
    } catch (error) {
      const errorMsg = String(error);
      setMessage(errorMsg);
      toast.danger("检查更新失败", { description: errorMsg });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      id={meta.anchorId}
      aria-labelledby={`${meta.anchorId}-title`}
      variant="outlined"
      className={`w-full min-w-0 max-w-full ${className ?? ""}`}
    >
      <div
        slot="headline"
        className="flex flex-wrap items-center justify-between gap-2 min-w-0 w-full"
      >
        <span
          id={`${meta.anchorId}-title`}
          className="flex flex-wrap items-center gap-2.5 min-w-0 font-semibold text-base"
        >
          <LuCpu aria-hidden="true" className="size-5 text-primary shrink-0" />
          <span>{meta.name}</span>
          <Chip size="sm" variant="soft" color="accent">
            {meta.category}
          </Chip>
        </span>
      </div>
      <div slot="subhead" className="mt-2 space-y-1.5 text-xs text-muted break-words">
        <div>
          <span className="font-medium text-foreground">适用功能：</span>
          <span>{meta.features}</span>
        </div>
        <div className="leading-relaxed">{meta.description}</div>
      </div>
      <div className="grid gap-4 w-full min-w-0 max-w-full p-4 overflow-hidden">
        {!status && (
          <div className="flex items-center gap-2 text-sm text-muted" role="status">
            <LoadingIndicator size={16} /> 正在检查模型状态
          </div>
        )}
        {status?.status === "ready" && (
          <Banner status="success" className="w-full min-w-0 max-w-full">
            <Banner.Title className="flex items-center gap-2 min-w-0">
              <LuCircleCheck aria-hidden="true" className="shrink-0" /> 模型已就绪
            </Banner.Title>
            <Banner.Description className="grid gap-1 min-w-0 w-full break-all">
              <div>{formatModelSize(status.bytes)} · 本机缓存</div>
              {status.sha256 && (
                <div className="text-xs text-muted font-mono break-all">
                  SHA-256: {status.sha256.slice(0, 16)}...
                </div>
              )}
            </Banner.Description>
          </Banner>
        )}
        {status?.status === "missing" && (
          <Banner status="warning" className="w-full min-w-0 max-w-full">
            <Banner.Title>尚未安装 {meta.category} 模型</Banner.Title>
            <Banner.Description className="break-words">
              首次下载约 {formatModelSize(status.bytes || meta.manifest.bytes)}
              ，完成后可离线使用。
            </Banner.Description>
          </Banner>
        )}
        {isDownloading && (
          <DownloadSession
            title={`正在下载并校验 ${meta.category} 模型...`}
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
        {status?.status === "error" && (
          <Banner status="danger" className="w-full min-w-0 max-w-full">
            <Banner.Title className="flex items-center gap-2 min-w-0">
              <LuTriangleAlert aria-hidden="true" className="shrink-0" /> 模型不可用
            </Banner.Title>
            <Banner.Description className="break-all max-w-full text-xs leading-relaxed font-mono whitespace-pre-wrap">
              {status.error ?? message ?? "下载或校验失败，请重试"}
            </Banner.Description>
          </Banner>
        )}
        {message && status?.status !== "error" && (
          <Banner status="danger" className="w-full min-w-0 max-w-full">
            <Banner.Description className="break-all max-w-full text-xs leading-relaxed font-mono whitespace-pre-wrap">
              {message}
            </Banner.Description>
          </Banner>
        )}
      </div>
      <div slot="actions" className="flex flex-wrap items-center gap-2 min-w-0 w-full p-4 pt-0">
        {!ready && (
          <Button
            disabled={busy || !status || status.status === "downloading" || busy}
            onClick={() => void handleDownload(false)}
          >
            <LuDownload aria-hidden="true" /> {downloadButtonLabel ?? meta.defaultDownloadLabel}
          </Button>
        )}
        {ready && (
          <>
            <Button
              variant="tonal"
              disabled={busy || isDownloading || busy}
              onClick={() => void handleCheckUpdate()}
            >
              <LuRefreshCw aria-hidden="true" /> 检查更新
            </Button>
            <Button
              variant="outlined"
              size="xs"
              disabled={busy || isDownloading || busy}
              onClick={() => void handleDownload(true)}
            >
              重新下载 / 修复
            </Button>
          </>
        )}
      </div>
    </Card>
  );
}

export function LocalModelsSettings({
  autoReturnKind,
  onboarding = false,
  onComplete,
  registerNext,
}: {
  autoReturnKind?: "ocr" | "embedding";
  onboarding?: boolean;
  onComplete?: () => void | Promise<void>;
  registerNext?: (action: StepNextAction | null) => void;
}) {
  useRegisterNextAction(
    registerNext,
    onboarding && onComplete ? { label: "下一步", run: onComplete } : null,
  );

  return (
    <div className="grid gap-6 w-full min-w-0 max-w-full">
      <div className="flex flex-col gap-1 min-w-0">
        <h3 className="text-base font-medium text-foreground">本地离线模型</h3>
        <p className="text-sm text-muted break-words">
          {onboarding
            ? "OCR 用于选课系统与 OJ 验证码识别，Embedding 用于 UOOC 题库检索。模型只保存在本机；暂时不需要可以跳过，之后在设置 → 本地模型中下载。"
            : "管理应用内置与离线运行的 AI / 机器学习模型。模型仅保存在本地设备，离线高速运行并保护隐私安全。"}
        </p>
      </div>
      <LocalModelCard
        kind="ocr"
        autoReturn={autoReturnKind === "ocr"}
        downloadButtonLabel="下载模型"
      />
      <LocalModelCard
        kind="embedding"
        autoReturn={autoReturnKind === "embedding"}
        downloadButtonLabel="下载 Embedding 模型"
      />
    </div>
  );
}
