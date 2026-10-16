import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { Alert, Button, Card, Spinner } from "@heroui/react";
import { LuCircleCheck, LuCpu, LuDownload, LuRefreshCw, LuTriangleAlert } from "react-icons/lu";
import { useLogicalNavigation } from "./logical-navigation";
import { ModelProgressBar, useModelDownloadProgress } from "./ModelDownloadProgress";

export type OcrModelStatus = {
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

export function useOcrModelStatus() {
  const [status, setStatus] = useState<OcrModelStatus>();
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<OcrModelStatus>("ocr-model-status-changed", (event) => {
      if (active) setStatus(event.payload);
    }).then((off) => {
      if (!active) off();
      else stop = off;
    });
    void invoke<OcrModelStatus>("get_ocr_model_status")
      .then((value) => active && setStatus(value))
      .catch(
        () =>
          active &&
          setStatus({
            status: "ready",
            revision: "",
            sha256: "",
            bytes: 0,
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

export function OcrModelSettings({ autoReturn = false }: { autoReturn?: boolean }) {
  const navigation = useLogicalNavigation();
  const status = useOcrModelStatus();
  const { progress } = useModelDownloadProgress("ocr");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>();
  const ready = status?.status === "ready";
  const isDownloading =
    busy || status?.status === "downloading" || (progress !== null && !ready);

  const downloadedBytes = progress?.downloadedBytes ?? status?.downloadedBytes ?? 0;
  const totalBytes = progress?.totalBytes ?? status?.totalBytes ?? status?.bytes ?? 2294200;
  const speedBytesPerSec = progress?.speedBytesPerSec ?? status?.speedBytesPerSec ?? 0;

  const download = async (update: boolean) => {
    setBusy(true);
    setMessage(undefined);
    try {
      await invoke<OcrModelStatus>(update ? "update_ocr_model" : "download_ocr_model");
      if (autoReturn) await navigation.back();
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card id="local-model-ocr" aria-labelledby="local-model-ocr-title">
      <Card.Header>
        <Card.Title id="local-model-ocr-title" className="flex items-center gap-2">
          <LuCpu aria-hidden="true" />
          本地模型-OCR
        </Card.Title>
        <Card.Description>
          用于选课和 OJ 验证码识别。模型保存在本机应用数据目录，不会上传账号信息。
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
                {status.source === "bundled" ? "安装包内置" : "本机缓存"}
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {status?.status === "missing" && (
          <Alert status="warning">
            <Alert.Content>
              <Alert.Title>尚未安装 OCR 模型</Alert.Title>
              <Alert.Description>
                首次下载约 {formatModelSize(status.bytes)}，完成后可离线识别验证码。
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {isDownloading && (
          <ModelProgressBar
            downloadedBytes={downloadedBytes}
            totalBytes={totalBytes}
            speedBytesPerSec={speedBytesPerSec}
            label="正在下载并校验 OCR 模型..."
          />
        )}
        {status?.status === "error" && (
          <Alert status="danger">
            <Alert.Content>
              <Alert.Title className="flex items-center gap-2">
                <LuTriangleAlert aria-hidden="true" /> 模型不可用
              </Alert.Title>
              <Alert.Description>{status.error ?? "下载或校验失败，请重试"}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {message && (
          <Alert status="danger">
            <Alert.Content>
              <Alert.Description>{message}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
      </Card.Content>
      <Card.Footer className="flex flex-wrap gap-2">
        {!ready && (
          <Button
            isPending={busy}
            isDisabled={busy || !status}
            onPress={() => void download(false)}
          >
            <LuDownload aria-hidden="true" /> 下载模型
          </Button>
        )}
        {ready && (
          <Button
            variant="secondary"
            isPending={busy}
            isDisabled={busy}
            onPress={() => void download(true)}
          >
            <LuRefreshCw aria-hidden="true" /> 检查更新
          </Button>
        )}
      </Card.Footer>
    </Card>
  );
}
