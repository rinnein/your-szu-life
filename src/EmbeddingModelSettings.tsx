import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useContext, useEffect, useState } from "react";
import { Alert, Button, Card, Spinner } from "@heroui/react";
import { LuCircleCheck, LuCpu, LuDownload, LuRefreshCw, LuTriangleAlert } from "react-icons/lu";
import { LogicalNavigationContext } from "./logical-navigation";
import { ModelProgressBar, useModelDownloadProgress } from "./ModelDownloadProgress";

export type EmbeddingModelStatus = {
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

export function formatModelSize(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function useEmbeddingModelStatus() {
  const [status, setStatus] = useState<EmbeddingModelStatus>();
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<EmbeddingModelStatus>("embedding-model-status-changed", (event) => {
      if (active) setStatus(event.payload);
    }).then((off) => {
      if (!active) off();
      else stop = off;
    });
    void invoke<EmbeddingModelStatus>("get_embedding_model_status")
      .then((value) => active && setStatus(value))
      .catch(
        () =>
          active &&
          setStatus({
            status: "ready",
            revision: "",
            sha256: "",
            bytes: 48942200,
            source: "web-preview",
          }),
      );
    return () => {
      active = false;
      stop?.();
    };
  }, []);
  return status;
}

export function EmbeddingModelSettings({ autoReturn = false }: { autoReturn?: boolean } = {}) {
  const navigation = useContext(LogicalNavigationContext);
  const status = useEmbeddingModelStatus();
  const { progress } = useModelDownloadProgress("embedding");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>();
  const ready = status?.status === "ready";
  const isDownloading =
    busy || status?.status === "downloading" || (progress !== null && !ready);

  const downloadedBytes = progress?.downloadedBytes ?? status?.downloadedBytes ?? 0;
  const totalBytes = progress?.totalBytes ?? status?.totalBytes ?? status?.bytes ?? 48942200;
  const speedBytesPerSec = progress?.speedBytesPerSec ?? status?.speedBytesPerSec ?? 0;

  const download = async (update: boolean) => {
    setBusy(true);
    setMessage(undefined);
    try {
      await invoke<EmbeddingModelStatus>(
        update ? "update_embedding_model" : "download_embedding_model",
      );
      if (autoReturn && navigation) await navigation.back();
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card id="local-model-embedding" aria-labelledby="local-model-embedding-title">
      <Card.Header>
        <Card.Title id="local-model-embedding-title" className="flex items-center gap-2">
          <LuCpu aria-hidden="true" />
          本地模型-Embedding
        </Card.Title>
        <Card.Description>
          用于文本语义嵌入与向量检索（BGE-small-zh-v1.5
          量化版）。模型保存在本机应用数据目录，支持离线高速向量特征计算。
        </Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-4">
        {!status && (
          <div className="flex items-center gap-2 text-sm text-muted" role="status">
            <Spinner size="sm" /> 正在检查模型状态
          </div>
        )}
        {status?.status === "ready" && (
          <Alert status="success">
            <Alert.Content>
              <Alert.Title className="flex items-center gap-2">
                <LuCircleCheck aria-hidden="true" /> 模型已就绪
              </Alert.Title>
              <Alert.Description>
                {formatModelSize(status.bytes)} ·{" "}
                {status.source === "bundled" ? "安装包内置" : "本机缓存"} (BGE-small-zh-v1.5 量化版)
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {status?.status === "missing" && (
          <Alert status="warning">
            <Alert.Content>
              <Alert.Title>尚未安装 Embedding 模型</Alert.Title>
              <Alert.Description>
                首次下载约 {formatModelSize(status.bytes)}，下载后可离线进行 BGE-small-zh-v1.5
                语义向量计算。
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {isDownloading && (
          <ModelProgressBar
            downloadedBytes={downloadedBytes}
            totalBytes={totalBytes}
            speedBytesPerSec={speedBytesPerSec}
            label="正在下载并校验 Embedding 模型..."
          />
        )}
        {status?.status === "error" && (
          <Alert status="danger">
            <Alert.Content>
              <Alert.Title className="flex items-center gap-2">
                <LuTriangleAlert aria-hidden="true" /> 模型下载失败
              </Alert.Title>
              <Alert.Description>{status.error || message || "网络连接异常"}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        <div className="flex flex-wrap gap-2">
          <Button
            variant={ready ? "secondary" : "primary"}
            isDisabled={busy || status?.status === "downloading"}
            onPress={() => void download(ready)}
          >
            {busy ? (
              <Spinner size="sm" />
            ) : ready ? (
              <LuRefreshCw aria-hidden="true" />
            ) : (
              <LuDownload aria-hidden="true" />
            )}
            {ready ? "重新下载 / 校验" : "下载 Embedding 模型"}
          </Button>
        </div>
      </Card.Content>
    </Card>
  );
}
