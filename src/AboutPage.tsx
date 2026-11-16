import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LoadingIndicator, Button, Card, Select, SelectOption } from "material/react";
import { Banner, toast } from "./ui";
import { LuArrowUpRight, LuDownload, LuRefreshCw } from "react-icons/lu";
import { BrandMark } from "./components";
import { useAppContent } from "./app-content/context";
import { ChangelogMarkdown } from "./app-content/ChangelogMarkdown";
import { contentTime, openPublicUrl } from "./app-content/external";
import { clearPostponedUpdateReminder } from "./app-content/update-postpone";
import { createTaskChannel } from "./tauri-channel";
import { getPlatform } from "./os-info";
import { DownloadSession } from "./DownloadSession";
import { errorMessage } from "./async-state";
import { usePending } from "./use-pending";
import { Contributors } from "./app-content/Contributors";
import type { AppContentSnapshot, UpdateChannel } from "./app-content/types";
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

/**
 * 应用内更新只有一个状态机，事件监听与命令回调都走同一个 `applyProgress`。
 * 之前两条管道各自 toast、各自清 `updating`，同一次更新会弹两次提示。
 */
type UpdateRun =
  | { status: "idle" }
  | { status: "downloading"; progress: UpdateProgress }
  | { status: "ready" }
  | { status: "error"; message: string };

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
  if (snapshot.update.data) {
    if (snapshot.updateAvailable) {
      return (
        "发现新版本 " +
        snapshot.update.data.latest_ver +
        (snapshot.update.fromCache ? "（缓存）" : "")
      );
    }
    if (snapshot.update.fromCache) return "上次检查未发现新版本（缓存）";
    return "当前已是最新版本";
  }
  if (snapshot.update.error) return "本次更新检查失败";
  return "尚未完成更新检查";
}

export default function AboutPage() {
  const { snapshot, error, refreshing, refresh } = useAppContent();
  const [platform, setPlatform] = useState<string>("unknown");
  const [updateRun, setUpdateRun] = useState<UpdateRun>({ status: "idle" });
  const updateRunRef = useRef<UpdateRun>({ status: "idle" });
  const pending = usePending();

  const applyProgress = useCallback((payload: UpdateProgress) => {
    const previous = updateRunRef.current;
    let next: UpdateRun;
    if (payload.status === "error") {
      next = {
        status: "error",
        message: payload.error?.trim() || "下载或解压过程中发生错误",
      };
    } else if (payload.status === "installing" || payload.status === "ready") {
      next = { status: "ready" };
    } else {
      next = { status: "downloading", progress: payload };
    }
    updateRunRef.current = next;
    setUpdateRun(next);
    // 每次状态迁移只提示一次：installing 之后紧跟的 ready 不会再弹第二次。
    if (next.status === "error" && previous.status !== "error") {
      toast.danger("应用内更新失败", { description: next.message });
    } else if (next.status === "ready" && previous.status !== "ready") {
      toast.success("下载完成", { description: "正在打开系统安装程序…" });
    }
  }, []);

  useEffect(() => {
    void getPlatform().then(setPlatform);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<UpdateProgress>("in-app-update-progress", (event) => {
      applyProgress(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [applyProgress]);

  const handleStartInAppUpdate = async () => {
    if (updateRunRef.current.status === "downloading" || pending.isPending("update")) return;
    applyProgress({
      status: "downloading",
      progress: 0,
      downloadedBytes: 0,
      totalBytes: 0,
      error: null,
    });
    const result = await pending.run("update", async () => {
      const channel = createTaskChannel<UpdateProgress>(applyProgress);
      await invoke("start_in_app_update", { onProgress: channel });
    });
    if (!result.ok && !result.skipped) {
      applyProgress({
        status: "error",
        progress: 0,
        downloadedBytes: 0,
        totalBytes: 0,
        error: errorMessage(result.error, "启动更新失败"),
      });
    }
  };

  const handleOpenLink = (url: string) => {
    void openPublicUrl(url).catch((cause) => {
      toast.danger("无法打开链接", { description: errorMessage(cause) });
    });
  };

  const update = snapshot?.update;
  const announcements = snapshot?.announcements;
  const configured = snapshot?.configuration.status === "ready";
  const hasUpdateSource =
    configured || (snapshot?.configuration.status === "unconfigured" && !!snapshot.repository);
  const channel: UpdateChannel = snapshot?.updateChannel ?? "stable";
  // 编译期自定义内容源与频道无关；快照未就绪时也不让用户盲切。
  const channelLocked = !snapshot || configured;
  const channelSaving = pending.isPending("channel");
  const updating = updateRun.status === "downloading" || pending.isPending("update");

  const handleChannelChange = async (value: string) => {
    const next: UpdateChannel = value === "canary" ? "canary" : "stable";
    if (next === channel || channelSaving) return;
    const result = await pending.run("channel", () =>
      invoke("set_update_channel", { channel: next }),
    );
    if (result.ok) {
      toast.success(next === "canary" ? "已切换到测试版更新" : "已切换到稳定版更新", {
        description: "正在按新频道检查更新…",
      });
    } else if (!result.skipped) {
      toast.danger("切换更新频道失败", {
        description: errorMessage(result.error, "请稍后重试。"),
      });
    }
  };

  // 作为设置页的「关于」tab 呈现，页面外壳与标题由设置页提供。
  return (
    <>
      <header className="about-header">
        <div className="about-brand">
          <BrandMark large />
          <div>
            <h2>泥深 · Your SZU Life</h2>
            <p className="content-muted">
              当前版本{" "}
              <strong data-testid="app-version">{snapshot?.version ?? packageInfo.version}</strong>
            </p>
          </div>
        </div>
      </header>
      <Card variant="outlined">
        <div slot="headline">更新与公告</div>
        <div slot="subhead">
          启动时自动检查,持续运行每 24 小时检查一次。手动检查同时刷新公告与更新信息
        </div>
        <div className="grid gap-4">
          <div>
            <Select
              label="更新频道"
              value={channel}
              disabled={channelLocked || channelSaving || refreshing}
              onChange={(event: any) => {
                const value = event.detail?.value ?? event.target?.value;
                if (value) void handleChannelChange(String(value));
              }}
            >
              <SelectOption value="stable">稳定版（latest）</SelectOption>
              <SelectOption value="canary">测试版（canary）</SelectOption>
            </Select>
            <p className="content-muted">
              {!snapshot
                ? "正在读取更新频道…"
                : channelLocked
                  ? "当前构建使用自定义内容源，频道切换不生效。"
                  : "稳定版只接收正式 release；测试版跟随 canary 构建。切换后会立即检查一次更新。"}
            </p>
            {snapshot?.updateChannelFallback && (
              <p className="content-muted" role="status">
                正式版暂无 Release，当前回退使用测试版构建。
              </p>
            )}
          </div>
          <div className="about-update-toolbar">
            <p className="update-status" role="status">
              {updateStatus(snapshot, refreshing, error)}
            </p>
            <Button
              variant="filled"
              disabled={refreshing || (!!snapshot && !hasUpdateSource)}
              onClick={() => {
                clearPostponedUpdateReminder();
                void refresh();
              }}
            >
              {refreshing ? <LoadingIndicator size={16} /> : <LuRefreshCw aria-hidden="true" />}
              {refreshing ? "正在检查…" : "检查更新"}
            </Button>
          </div>
          {snapshot?.configuration.message && (
            <p className="content-muted">{snapshot.configuration.message}</p>
          )}
          {(error || update?.error) && (
            <Banner status="warning" role="status">
              <Banner.Title>未能确认最新版本</Banner.Title>
              <Banner.Description>
                {error || update?.error?.message}
                {update?.fromCache &&
                  " 以下版本信息与更新日志来自上次成功检查，不代表本次检查成功。"}
              </Banner.Description>
            </Banner>
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
                    : update?.source === "cnb"
                      ? " · CNB 镜像"
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
                    : announcements?.source === "cnb"
                      ? " · CNB 镜像"
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
          {hasUpdateSource && !refreshing && announcements?.data && (
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
              {update.data.changelog ? (
                <ChangelogMarkdown source={update.data.changelog} />
              ) : (
                <p className="content-plaintext">此版本未提供更新日志。</p>
              )}
            </section>
          )}
          {snapshot?.updateAvailable && update?.data && (
            <div className="grid gap-3">
              {platform === "android" || platform === "windows" ? (
                <>
                  {updateRun.status === "downloading" && (
                    <DownloadSession
                      title={
                        platform === "windows"
                          ? `正在下载新版本安装程序 ${update.data.latest_ver}...`
                          : `正在下载并解压新版本 ${update.data.latest_ver}...`
                      }
                      defaultExpanded
                      session={{
                        downloadedBytes: updateRun.progress.downloadedBytes,
                        totalBytes: updateRun.progress.totalBytes,
                        speedBytesPerSec: updateRun.progress.speedBytesPerSec,
                        port: updateRun.progress.port,
                        numPieces: updateRun.progress.numPieces,
                        pieceLength: updateRun.progress.pieceLength,
                        bitfield: updateRun.progress.bitfield,
                        connections: updateRun.progress.connections,
                        isResumed: updateRun.progress.isResumed,
                        resumedBytes: updateRun.progress.resumedBytes,
                        totalActiveTasks: updateRun.progress.totalActiveTasks,
                        status: updateRun.progress.status,
                        error: updateRun.progress.error,
                      }}
                    />
                  )}
                  {updateRun.status === "ready" && (
                    <p className="content-muted" role="status">
                      <LoadingIndicator size={16} /> 正在启动安装程序，请按系统提示完成更新…
                    </p>
                  )}
                  {updateRun.status === "error" && (
                    <Banner status="danger" role="alert">
                      <Banner.Title>应用内更新失败</Banner.Title>
                      <Banner.Description>{updateRun.message}</Banner.Description>
                    </Banner>
                  )}
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="filled"
                      disabled={updating}
                      onClick={() => void handleStartInAppUpdate()}
                    >
                      {updating ? (
                        <LoadingIndicator size={16} />
                      ) : (
                        <LuDownload aria-hidden="true" />
                      )}
                      {updating
                        ? "正在下载…"
                        : updateRun.status === "error"
                          ? platform === "windows"
                            ? `重试下载安装 ${update.data.latest_ver}`
                            : `重试应用内更新 ${update.data.latest_ver}`
                          : platform === "windows"
                            ? `下载并安装 ${update.data.latest_ver}`
                            : `应用内更新 ${update.data.latest_ver}`}
                    </Button>
                    <Button
                      variant="tonal"
                      onClick={() => handleOpenLink(update.data!.latest_ver_url)}
                    >
                      前往浏览器下载
                      <LuArrowUpRight aria-hidden="true" />
                    </Button>
                  </div>
                </>
              ) : platform === "linux" ? (
                <Button
                  variant="tonal"
                  onClick={() =>
                    handleOpenLink(update.data!.release_url || update.data!.latest_ver_url)
                  }
                >
                  <LuDownload aria-hidden="true" /> 前往发布页面 {update.data.latest_ver}
                  <LuArrowUpRight aria-hidden="true" />
                </Button>
              ) : (
                <Button variant="tonal" onClick={() => handleOpenLink(update.data!.latest_ver_url)}>
                  <LuDownload aria-hidden="true" /> 前往下载 {update.data.latest_ver}
                  <LuArrowUpRight aria-hidden="true" />
                </Button>
              )}
            </div>
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
      <Card variant="outlined">
        <div slot="headline">UOOC 功能参考</div>
        <div slot="subhead">
          视频流程参考 auto-uooc-go，测验交互参考 fuckuooc，使用 Rust 独立实现
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="text"
            onClick={() => handleOpenLink("https://github.com/rinnein/auto-uooc-go")}
          >
            auto-uooc-go <LuArrowUpRight aria-hidden="true" />
          </Button>
          <Button
            variant="text"
            onClick={() => handleOpenLink("https://github.com/YusongXiao/fuckuooc")}
          >
            fuckuooc <LuArrowUpRight aria-hidden="true" />
          </Button>
        </div>
      </Card>
    </>
  );
}
