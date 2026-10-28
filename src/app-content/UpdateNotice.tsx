import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useRouterState } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Dialog, LoadingIndicator, Button } from "material/react";
import { Banner, toast } from "../ui";
import { LuArrowUpRight, LuDownload } from "react-icons/lu";
import { useAppContent } from "./context";
import { openPublicUrl } from "./external";
import { getPlatform } from "../os-info";
import { isUpdatePostponed, postponeUpdateReminder } from "./update-postpone";
import { createTaskChannel } from "../tauri-channel";
import { DownloadSession } from "../DownloadSession";

type UpdateProgress = {
  status: "downloading" | "installing" | "ready" | "error";
  progress: number;
  downloadedBytes: number;
  totalBytes: number;
  speedBytesPerSec?: number;
  port?: number;
  numPieces?: number;
  pieceLength?: number;
  bitfield?: string | null;
  connections?: number;
  isResumed?: boolean;
  resumedBytes?: number;
  totalActiveTasks?: number;
  error: string | null;
};

export function UpdateNotice() {
  const { snapshot } = useAppContent();
  const location = useRouterState({
    select: (state) => ({
      pathname: state.location.pathname,
      searchStr: state.location.searchStr,
    }),
  });
  const [isOpen, setIsOpen] = useState(false);
  const [platform, setPlatform] = useState<string>("unknown");
  const [updating, setUpdating] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);

  const latest = snapshot?.update.data?.latest_ver;
  const updateData = snapshot?.update.data;

  useEffect(() => {
    void getPlatform().then(setPlatform);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<UpdateProgress>("in-app-update-progress", (event) => {
      setUpdateProgress(event.payload);
      if (event.payload.status === "error") {
        setUpdating(false);
        toast.danger("应用内更新失败", {
          description: event.payload.error ?? "下载或解压过程中发生错误",
        });
      } else if (event.payload.status === "installing" || event.payload.status === "ready") {
        setUpdating(false);
        toast.success("下载完成", {
          description: "正在打开系统安装程序…",
        });
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    // 设置页的「关于」tab 自带更新面板，那里不再重复弹窗。
    const onAboutTab =
      location.pathname === "/settings" &&
      new URLSearchParams(location.searchStr ?? "").get("tab") === "about";
    const shouldShow =
      Boolean(snapshot?.updateAvailable) && Boolean(latest) && !isUpdatePostponed() && !onAboutTab;

    setIsOpen(shouldShow);
  }, [snapshot?.updateAvailable, latest, location.pathname, location.searchStr]);

  const handlePostpone = () => {
    postponeUpdateReminder();
    setIsOpen(false);
  };

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      handlePostpone();
    } else {
      setIsOpen(true);
    }
  };

  const handleStartInAppUpdate = async () => {
    setUpdating(true);
    setUpdateProgress({
      status: "downloading",
      progress: 0,
      downloadedBytes: 0,
      totalBytes: 0,
      error: null,
    });
    try {
      const channel = createTaskChannel<UpdateProgress>((progress) => {
        setUpdateProgress(progress);
        if (progress.status === "error") {
          setUpdating(false);
          toast.danger("应用内更新失败", {
            description: progress.error ?? "下载或解压过程中发生错误",
          });
        } else if (progress.status === "installing" || progress.status === "ready") {
          setUpdating(false);
          toast.success("下载完成", {
            description: "正在打开系统安装程序…",
          });
        }
      });
      await invoke("start_in_app_update", { onProgress: channel });
    } catch (e) {
      setUpdating(false);
      const msg = typeof e === "string" ? e : "启动更新失败";
      setUpdateProgress({
        status: "error",
        progress: 0,
        downloadedBytes: 0,
        totalBytes: 0,
        error: msg,
      });
      toast.danger("更新失败", { description: msg });
    }
  };

  const handleOpenReleasePage = () => {
    if (!updateData) return;
    const url = updateData.release_url || updateData.latest_ver_url;
    void openPublicUrl(url).catch(() => {
      toast.danger("无法打开系统浏览器，请稍后重试。");
    });
    handlePostpone();
  };

  const handleOpenBrowserDownload = () => {
    if (!updateData) return;
    void openPublicUrl(updateData.latest_ver_url).catch(() => {
      toast.danger("无法打开系统浏览器，请稍后重试。");
    });
  };

  if (!isOpen || !updateData) return null;

  const isAndroid = platform === "android";
  const isWindows = platform === "windows";
  const isLinux = platform === "linux";
  const isDownloading = updateProgress?.status === "downloading";
  const isInstalling =
    updateProgress?.status === "installing" || updateProgress?.status === "ready";

  return createPortal(
    <Dialog
      open={isOpen}
      headline={`发现新版本 ${latest}`}
      className="max-w-lg"
      onClose={() => handleOpenChange(false)}
    >
      <div className="space-y-3">
        <p className="text-sm text-muted">
          {snapshot?.update.fromCache
            ? "以下版本信息与更新日志来自上次检查缓存。"
            : "检测到新版本可用，建议更新以体验最新功能与修复。"}
        </p>

        {updateData.changelog ? (
          <div className="max-h-48 overflow-y-auto rounded-lg bg-surface p-3 text-xs leading-relaxed text-muted whitespace-pre-wrap border border-separator">
            <div className="font-semibold text-foreground mb-1">更新日志：</div>
            {updateData.changelog}
          </div>
        ) : (
          <p className="text-xs text-muted">此版本未提供更新日志。</p>
        )}

        {isDownloading && updateProgress && (
          <div className="pt-2">
            <DownloadSession
              title={`正在下载新版本 ${latest}...`}
              defaultExpanded
              session={{
                downloadedBytes: updateProgress.downloadedBytes,
                totalBytes: updateProgress.totalBytes,
                speedBytesPerSec: updateProgress.speedBytesPerSec,
                port: updateProgress.port,
                numPieces: updateProgress.numPieces,
                pieceLength: updateProgress.pieceLength,
                bitfield: updateProgress.bitfield,
                connections: updateProgress.connections,
                isResumed: updateProgress.isResumed,
                resumedBytes: updateProgress.resumedBytes,
                totalActiveTasks: updateProgress.totalActiveTasks,
                status: updateProgress.status,
                error: updateProgress.error,
              }}
            />
          </div>
        )}

        {isInstalling && (
          <p className="text-sm text-muted flex items-center gap-2 pt-2" role="status">
            <LoadingIndicator size={16} /> 正在启动安装程序，请按提示完成更新…
          </p>
        )}

        {updateProgress?.status === "error" && (
          <Banner status="danger" role="alert">
            <Banner.Title>更新失败</Banner.Title>
            <Banner.Description>
              {updateProgress.error ?? "下载或解压过程中发生错误。"}
            </Banner.Description>
          </Banner>
        )}
      </div>

      <div slot="actions" className="flex flex-wrap items-center justify-end gap-2">
        <Button variant="text" onClick={handlePostpone}>
          稍后再说
        </Button>

        {isWindows && (
          <Button
            variant="filled"
            disabled={updating || isDownloading}
            onClick={() => void handleStartInAppUpdate()}
          >
            {updating || isDownloading ? (
              <LoadingIndicator size={16} />
            ) : (
              <LuDownload aria-hidden="true" />
            )}
            {updating || isDownloading
              ? "正在下载…"
              : updateProgress?.status === "error"
                ? "重试下载安装"
                : "下载并安装"}
          </Button>
        )}

        {isAndroid && (
          <>
            <Button
              variant="filled"
              disabled={updating || isDownloading}
              onClick={() => void handleStartInAppUpdate()}
            >
              {updating || isDownloading ? (
                <LoadingIndicator size={16} />
              ) : (
                <LuDownload aria-hidden="true" />
              )}
              {updating || isDownloading
                ? "正在下载…"
                : updateProgress?.status === "error"
                  ? "重试下载安装"
                  : "应用内更新"}
            </Button>
            {!isDownloading && (
              <Button variant="tonal" onClick={handleOpenBrowserDownload}>
                浏览器下载
                <LuArrowUpRight aria-hidden="true" />
              </Button>
            )}
          </>
        )}

        {isLinux && (
          <Button variant="filled" onClick={handleOpenReleasePage}>
            前往发布页面
            <LuArrowUpRight aria-hidden="true" />
          </Button>
        )}

        {!isWindows && !isAndroid && !isLinux && (
          <Button variant="filled" onClick={handleOpenBrowserDownload}>
            前往下载
            <LuArrowUpRight aria-hidden="true" />
          </Button>
        )}
      </div>
    </Dialog>,
    document.body,
  );
}
