import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Alert, Avatar, Button, Spinner, toast } from "@heroui/react";
import { LuArrowUpRight, LuDownload, LuRefreshCw, LuUsers } from "react-icons/lu";
import { BrandMark, Card, ModuleHeader, PageShell } from "./components";
import { useAppContent } from "./app-content/context";
import { contentTime, openPublicUrl } from "./app-content/external";
import { clearPostponedUpdateReminder } from "./app-content/update-postpone";
import { createTaskChannel } from "./tauri-channel";
import { getPlatform } from "./os-info";
import { DownloadSession } from "./DownloadSession";
import type { AppContentSnapshot, ContributorsSnapshot } from "./app-content/types";
import packageInfo from "../package.json";

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

export function updateStatus(
  snapshot: AppContentSnapshot | undefined,
  refreshing: boolean,
  error?: string,
) {
  if (refreshing) return "正在检查公告和更新…";
  if (error) return "本次检查未能完成";
  if (!snapshot) return "正在读取更新状态…";
  if (snapshot.configuration.status === "unconfigured" && !snapshot.repository)
    return "未配置内容源";
  if (snapshot.configuration.status === "invalid") return "内容源配置无效";
  if (snapshot.update.error) return "本次更新检查失败";
  if (!snapshot.update.data) return "尚未完成更新检查";
  if (snapshot.updateAvailable) return "发现新版本 " + snapshot.update.data.latest_ver;
  if (snapshot.update.fromCache) return "上次检查未发现新版本（缓存）";
  return "当前已是最新版本";
}

function Contributors() {
  const [result, setResult] = useState<ContributorsSnapshot>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [linkError, setLinkError] = useState<string>();
  const [generation, setGeneration] = useState(0);
  const request = useRef<Promise<ContributorsSnapshot> | undefined>(undefined);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(undefined);
    // StrictMode reuses the same request; only entering About or explicitly retrying loads contributors.
    request.current ??= invoke<ContributorsSnapshot>("get_contributors");
    void request.current
      .then((next) => {
        if (!active) return;
        if (!next) {
          setError("无法读取贡献者信息，请在原生应用中重试。");
          return;
        }
        setResult(next);
      })
      .catch(() => {
        if (active) setError("贡献者加载失败，请检查网络后重试。");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [generation]);

  const open = (url: string) => {
    setLinkError(undefined);
    void openPublicUrl(url).catch(() => setLinkError("无法打开系统浏览器，请稍后重试。"));
  };

  return (
    <Card
      title="贡献者"
      icon={<LuUsers />}
      description="感谢每一位参与者。列表来自公开 GitHub 仓库，按仓库缓存 24 小时"
    >
      <div className="grid gap-4">
        {result?.repository && (
          <Button
            className="repository-link"
            size="sm"
            variant="tertiary"
            onPress={() => open(result.repository!.url)}
          >
            {result.repository.fullName}
            <LuArrowUpRight aria-hidden="true" />
          </Button>
        )}
        {loading && (
          <p className="content-loading" role="status">
            <Spinner size="sm" /> 正在加载贡献者…
          </p>
        )}
        {result?.status === "unconfigured" && (
          <p className="content-muted" role="status">
            {result.message}
          </p>
        )}
        {(error || result?.error) && (
          <Alert status="warning" role="status">
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Title>
                {result?.status === "rateLimited" ? "GitHub 请求受限" : "贡献者加载失败"}
              </Alert.Title>
              <Alert.Description>
                {error || result?.error?.message}
                {result?.fromCache && " 仍显示上次成功加载的贡献者。"}
                {result?.error?.retryAt &&
                  " 可在 " + contentTime(result.error.retryAt) + " 后重试。"}
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {!!result?.contributors.length && (
          <ul className="contributors-grid" aria-label="GitHub 贡献者">
            {result.contributors.map((contributor) => (
              <li key={contributor.login}>
                <Button
                  className="contributor-link"
                  variant="tertiary"
                  onPress={() => open(contributor.url)}
                  aria-label={contributor.login + " 的 GitHub 主页"}
                >
                  <Avatar size="sm">
                    <Avatar.Image
                      src={contributor.avatarUrl}
                      alt={contributor.login + "头像"}
                      loading="lazy"
                      crossOrigin="anonymous"
                      referrerPolicy="no-referrer"
                    />
                    <Avatar.Fallback>{contributor.login.slice(0, 2).toUpperCase()}</Avatar.Fallback>
                  </Avatar>
                  <span>{contributor.login}</span>
                </Button>
              </li>
            ))}
          </ul>
        )}
        {!loading && result?.status === "ready" && result.contributors.length === 0 && (
          <p className="content-muted">该仓库暂无贡献者。</p>
        )}
        {result?.truncated && (
          <p className="content-muted">仅展示前 1,000 位贡献者，完整列表请前往 GitHub。</p>
        )}
        {result?.fromCache && (
          <p className="content-muted">缓存于 {contentTime(result.lastSuccessAt)}</p>
        )}
        {(error || result?.error) && (
          <Button
            variant="secondary"
            size="sm"
            isDisabled={loading}
            onPress={() => {
              request.current = undefined;
              setGeneration((value) => value + 1);
            }}
          >
            重新加载贡献者
          </Button>
        )}
        {linkError && (
          <p role="alert" className="content-error">
            {linkError}
          </p>
        )}
      </div>
    </Card>
  );
}

export default function AboutPage() {
  const { snapshot, error, refreshing, refresh } = useAppContent();
  const [openError, setOpenError] = useState<string>();
  const [platform, setPlatform] = useState<string>("unknown");
  const [updating, setUpdating] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);

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
      toast.danger("应用内更新失败", { description: msg });
    }
  };

  const update = snapshot?.update;
  const announcements = snapshot?.announcements;
  const configured = snapshot?.configuration.status === "ready";
  const hasUpdateSource =
    configured || (snapshot?.configuration.status === "unconfigured" && !!snapshot.repository);
  return (
    <PageShell>
      <div className="about-layout">
        <ModuleHeader title="关于" />
        <header className="about-header">
          <span data-webview-navigation className="eyebrow">
            ABOUT YOUR SZU LIFE
          </span>
          <h1 data-webview-navigation className="sr-only">
            关于
          </h1>
          <div className="about-brand">
            <BrandMark large />
            <div>
              <h2>泥深 · Your SZU Life</h2>
              <p className="content-muted">
                当前版本{" "}
                <strong data-testid="app-version">
                  {snapshot?.version ?? packageInfo.version}
                </strong>
              </p>
            </div>
          </div>
        </header>
        <Card
          title="更新与公告"
          description="启动时自动检查,持续运行每 24 小时检查一次。手动检查同时刷新公告与更新信息"
        >
          <div className="grid gap-4">
            <div className="about-update-toolbar">
              <p className="update-status" role="status">
                {updateStatus(snapshot, refreshing, error)}
              </p>
              <Button
                variant="primary"
                isDisabled={refreshing || (!!snapshot && !hasUpdateSource)}
                onPress={() => {
                  clearPostponedUpdateReminder();
                  void refresh();
                }}
              >
                {refreshing ? (
                  <Spinner size="sm" color="current" />
                ) : (
                  <LuRefreshCw aria-hidden="true" />
                )}
                {refreshing ? "正在检查…" : "检查更新"}
              </Button>
            </div>
            {snapshot?.configuration.message && (
              <p className="content-muted">{snapshot.configuration.message}</p>
            )}
            {(error || update?.error) && (
              <Alert status="warning" role="status">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>未能确认最新版本</Alert.Title>
                  <Alert.Description>
                    {error || update?.error?.message}
                    {update?.fromCache &&
                      " 以下版本信息与更新日志来自上次成功检查，不代表本次检查成功。"}
                  </Alert.Description>
                </Alert.Content>
              </Alert>
            )}
            <dl className="content-times">
              <div>
                <dt>最近检查</dt>
                <dd>{contentTime(update?.lastAttemptAt)}</dd>
              </div>
              <div>
                <dt>更新信息最后成功</dt>
                <dd>
                  {contentTime(update?.lastSuccessAt)}
                  {update?.source === "fallback"
                    ? " · 备用源"
                    : update?.source === "github"
                      ? " · GitHub"
                      : ""}
                </dd>
              </div>
              <div>
                <dt>公告最后成功</dt>
                <dd>
                  {contentTime(announcements?.lastSuccessAt)}
                  {announcements?.source === "fallback"
                    ? " · 备用源"
                    : announcements?.source === "github"
                      ? " · GitHub"
                      : ""}
                </dd>
              </div>
              {snapshot?.nextAutomaticCheckAt != null && (
                <div>
                  <dt>下次自动检查</dt>
                  <dd>{contentTime(snapshot.nextAutomaticCheckAt)}</dd>
                </div>
              )}
            </dl>
            {announcements?.error && (
              <p className="content-error" role="status">
                公告获取失败：{announcements.error.message}
                {announcements.fromCache && " 已保留上次成功的公告。"}
              </p>
            )}
            {hasUpdateSource && !refreshing && announcements?.data && !announcements.error && (
              <p className="content-muted">
                {announcements.fromCache ? "公告缓存" : "公告已刷新"} ·{" "}
                {announcements.data.announcements.length} 条
              </p>
            )}
            {update?.data && (
              <section className="changelog" aria-labelledby="changelog-heading">
                <h3 id="changelog-heading">
                  更新日志 · {update.data.latest_ver}
                  {update.fromCache ? "（缓存）" : ""}
                </h3>
                <p className="content-plaintext">
                  {update.data.changelog || "此版本未提供更新日志。"}
                </p>
              </section>
            )}
            {snapshot?.updateAvailable && update?.data && (
              <div className="grid gap-3">
                {platform === "android" || platform === "windows" ? (
                  <>
                    {updateProgress?.status === "downloading" && (
                      <DownloadSession
                        title={
                          platform === "windows"
                            ? `正在下载新版本安装程序 ${update.data.latest_ver}...`
                            : `正在下载并解压新版本 ${update.data.latest_ver}...`
                        }
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
                    )}
                    {(updateProgress?.status === "installing" ||
                      updateProgress?.status === "ready") && (
                      <p className="content-muted" role="status">
                        <Spinner size="sm" /> 正在启动安装程序，请按系统提示完成更新…
                      </p>
                    )}
                    {updateProgress?.status === "error" && (
                      <Alert status="danger" role="alert">
                        <Alert.Indicator />
                        <Alert.Content>
                          <Alert.Title>应用内更新失败</Alert.Title>
                          <Alert.Description>
                            {updateProgress.error ?? "下载或解压过程中发生错误。"}
                          </Alert.Description>
                        </Alert.Content>
                      </Alert>
                    )}
                    <div className="flex flex-wrap gap-2">
                      <Button
                        variant="primary"
                        isDisabled={updating || updateProgress?.status === "downloading"}
                        onPress={() => void handleStartInAppUpdate()}
                      >
                        {updating || updateProgress?.status === "downloading" ? (
                          <Spinner size="sm" color="current" />
                        ) : (
                          <LuDownload aria-hidden="true" />
                        )}
                        {updating || updateProgress?.status === "downloading"
                          ? "正在下载…"
                          : updateProgress?.status === "error"
                            ? platform === "windows"
                              ? `重试下载安装 ${update.data.latest_ver}`
                              : `重试应用内更新 ${update.data.latest_ver}`
                            : platform === "windows"
                              ? `下载并安装 ${update.data.latest_ver}`
                              : `应用内更新 ${update.data.latest_ver}`}
                      </Button>
                      <Button
                        variant="secondary"
                        onPress={() => {
                          setOpenError(undefined);
                          void openPublicUrl(update.data!.latest_ver_url).catch(() =>
                            setOpenError("无法打开系统浏览器，请稍后重试。"),
                          );
                        }}
                      >
                        前往浏览器下载
                        <LuArrowUpRight aria-hidden="true" />
                      </Button>
                    </div>
                  </>
                ) : platform === "linux" ? (
                  <Button
                    variant="secondary"
                    onPress={() => {
                      setOpenError(undefined);
                      const targetUrl = update.data!.release_url || update.data!.latest_ver_url;
                      void openPublicUrl(targetUrl).catch(() =>
                        setOpenError("无法打开系统浏览器，请稍后重试。"),
                      );
                    }}
                  >
                    <LuDownload aria-hidden="true" /> 前往发布页面 {update.data.latest_ver}
                    <LuArrowUpRight aria-hidden="true" />
                  </Button>
                ) : (
                  <Button
                    variant="secondary"
                    onPress={() => {
                      setOpenError(undefined);
                      void openPublicUrl(update.data!.latest_ver_url).catch(() =>
                        setOpenError("无法打开系统浏览器，请稍后重试。"),
                      );
                    }}
                  >
                    <LuDownload aria-hidden="true" /> 前往下载 {update.data.latest_ver}
                    <LuArrowUpRight aria-hidden="true" />
                  </Button>
                )}
              </div>
            )}
            {openError && (
              <p className="content-error" role="alert">
                {openError}
              </p>
            )}
            <p className="content-muted about-download-note">
              {platform === "android"
                ? "Android 端支持应用内流式解压更新与加速节点，也可以点击前往系统浏览器下载。"
                : platform === "windows"
                  ? "Windows 端支持应用内下载 msi 安装程序与加速节点，下载完成后自动执行安装。"
                  : platform === "linux"
                    ? "Linux 端建议通过包管理器自动更新，也可点击前往发布页面手动获取安装包。"
                    : "仅检查版本，不会自动下载或安装；点击下载后在系统浏览器打开。"}
            </p>
          </div>
        </Card>
        <Contributors />
        <Card
          title="UOOC 功能参考"
          description="视频流程参考 auto-uooc-go，测验交互参考 fuckuooc，使用 Rust 独立实现"
        >
          <div className="flex flex-wrap gap-2">
            <Button
              variant="tertiary"
              onPress={() =>
                void openPublicUrl("https://github.com/rinnein/auto-uooc-go").catch(() =>
                  setOpenError("无法打开参考项目链接"),
                )
              }
            >
              auto-uooc-go <LuArrowUpRight aria-hidden="true" />
            </Button>
            <Button
              variant="tertiary"
              onPress={() =>
                void openPublicUrl("https://github.com/YusongXiao/fuckuooc").catch(() =>
                  setOpenError("无法打开参考项目链接"),
                )
              }
            >
              fuckuooc <LuArrowUpRight aria-hidden="true" />
            </Button>
          </div>
        </Card>
      </div>
    </PageShell>
  );
}
