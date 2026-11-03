import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { useJxxkDownloads } from "./useDownloads";
import { MediaLoader, mediaLoaderConfig, toMediaUrl } from "./mediaLoader";
import { createSyncPlugin } from "./artSync";
import { DownloadManagerView } from "./DownloadManagerPage";
import { TopAppBar } from "../shell/TopAppBar";
import { createPortal } from "react-dom";
import {
  Button,
  Card,
  Checkbox,
  Dialog,
  LoadingIndicator,
  Progress,
  Toolbar,
} from "material/react";
import {
  LuDownload,
  LuMonitor,
  LuRotateCw,
  LuSquarePlay,
  LuUser,
  LuUsers,
  LuVideo,
} from "react-icons/lu";
import { Banner, toast } from "../ui";
import { useFileManagerState } from "../file-manager/useFileManagerState";
import { PageSurface } from "../shell/PageSurface";
import { AuthGate } from "../shell/AuthGate";
import Artplayer from "artplayer";
import Hls from "hls.js";
import {
  downloadUrls,
  invokeDownload,
  lessonStreams,
  liveStream,
  playUrls,
  subtitle as fetchSubtitle,
  type JxxkLessonStreams,
  type JxxkPlayUrlScreen,
  type JxxkLiveStreamSource,
} from "./api";

/** 统一的画面描述：每个码流一个 ArtPlayer 实例。 */
interface Angle {
  key: string;
  name: string;
  playUrl: string;
}

function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 100 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function formatDuration(seconds: number | null | undefined): string {
  if (!seconds || seconds <= 0) return "—";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
}

/** 码流名称 → 工具栏图标。 */
function streamIcon(name: string): React.ReactNode {
  if (name.includes("教师")) return <LuUser aria-hidden="true" />;
  if (name.includes("学生")) return <LuUsers aria-hidden="true" />;
  if (name.includes("电脑")) return <LuMonitor aria-hidden="true" />;
  if (name.includes("主")) return <LuSquarePlay aria-hidden="true" />;
  return <LuVideo aria-hidden="true" />;
}

/** video 元素到其 hls.js 实例的关联，用于切换/卸载时释放。 */
const hlsInstances = new WeakMap<HTMLVideoElement, Hls>();

/** 上一次选择的码流（streamNo 列表），跨选集记忆。 */
const LAST_STREAMS_KEY = "jxxk-last-streams";
function loadLastStreams(): string[] {
  try {
    const raw = localStorage.getItem(LAST_STREAMS_KEY);
    const parsed = raw ? (JSON.parse(raw) as unknown) : [];
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}
function saveLastStreams(keys: string[]): void {
  try {
    localStorage.setItem(LAST_STREAMS_KEY, JSON.stringify(keys));
  } catch {
    // 忽略持久化失败
  }
}

/** 播放器设置持久化：默认静音与倍速。 */
interface PlayerSettings {
  muted: boolean;
  rate: number;
}
const PLAYER_SETTINGS_KEY = "jxxk-player-settings";
function loadPlayerSettings(): PlayerSettings {
  try {
    const raw = localStorage.getItem(PLAYER_SETTINGS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<PlayerSettings>;
      return {
        muted: parsed.muted === true,
        rate: typeof parsed.rate === "number" && parsed.rate > 0 ? parsed.rate : 1,
      };
    }
  } catch {
    // 忽略
  }
  return { muted: false, rate: 1 };
}
function savePlayerSettings(settings: PlayerSettings): void {
  try {
    localStorage.setItem(PLAYER_SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    // 忽略
  }
}

/** 把媒体地址交给 hls.js（或原生 HLS）播放；致命错误经 onError 上浮。 */
function attachHls(
  video: HTMLVideoElement,
  url: string,
  art?: Artplayer,
  onError?: (message: string) => void,
): void {
  if (Hls.isSupported()) {
    const hls = new Hls({ enableWorker: true, ...mediaLoaderConfig() });
    hls.loadSource(url);
    hls.attachMedia(video);
    hls.on(Hls.Events.ERROR, (_event, data) => {
      if (data.fatal) {
        const detail =
          (data.response?.data as string | undefined) || MediaLoader.lastError || data.reason;
        onError?.(`HLS ${data.type} 错误：${data.details}${detail ? `（${detail}）` : ""}`);
      }
    });
    hlsInstances.set(video, hls);
    if (art) {
      (art as unknown as { hls?: Hls }).hls = hls;
      art.on("destroy", () => hls.destroy());
    }
  } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
    video.src = url;
  } else {
    onError?.("当前 WebView 不支持 HLS 播放");
  }
}

/** 建一个同步组内的 ArtPlayer 实例：全部等大同配置（仅声音默认给第一个）。 */
function createPlayer(
  container: HTMLDivElement,
  angle: Angle,
  options: {
    syncGroupId: string;
    subtitleUrl: string | null;
    muted: boolean;
    rate: number;
    isLive: boolean;
    onError?: (message: string) => void;
  },
): Artplayer {
  return new Artplayer({
    container,
    url: toMediaUrl(angle.playUrl),
    type: "m3u8",
    isLive: options.isLive,
    customType: {
      m3u8: (video: HTMLVideoElement, url: string, player: Artplayer) => {
        attachHls(video, url, player, options.onError);
      },
    },
    ...(options.subtitleUrl
      ? { subtitle: { url: options.subtitleUrl, type: "vtt", encoding: "utf-8" } }
      : {}),
    plugins: [createSyncPlugin(options.syncGroupId)],
    theme: "var(--mtrl-sys-color-primary)",
    volume: 0.7,
    muted: options.muted,
    setting: true,
    aspectRatio: true,
    fullscreen: true,
    fullscreenWeb: true,
    lock: false,
    pip: true,
  });
}

export default function WatchPage() {
  const { liveId, course, semesterName, teacher, title, liveState } = useSearch({
    from: "/study/patrol_/watch",
  });
  const containerRefs = useRef<Map<string, HTMLDivElement>>(new Map());
  const artRefs = useRef<Map<string, Artplayer>>(new Map());
  const subtitleBlobRef = useRef<string | null>(null);
  const subtitleTextRef = useRef<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [streams, setStreams] = useState<JxxkLessonStreams | null>(null);
  const [angles, setAngles] = useState<Angle[]>([]);
  const [visible, setVisible] = useState<string[]>([]);
  const [subtitleUrl, setSubtitleUrl] = useState<string | null>(null);
  const { tasks: downloads } = useJxxkDownloads();
  const { state: fileManagerState } = useFileManagerState();
  const downloadDir = fileManagerState?.modules.find((module) => module.id === "jxxk")?.path;
  const [confirmStream, setConfirmStream] = useState<{
    name: string;
    streamNo: string | null;
  } | null>(null);
  const [retryToken, setRetryToken] = useState(0);
  const [playerError, setPlayerError] = useState<string | null>(null);
  const [managerOpen, setManagerOpen] = useState(false);

  const isLive = liveState === 2;
  // 同步组：同一次录像内的全部码流实例共享进度。
  const syncGroupId = streams?.mediaVideoId || liveId;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setAngles([]);
    setStreams(null);
    setVisible([]);
    setPlayerError(null);
    (async () => {
      try {
        if (isLive) {
          let list: Angle[] = [];
          // 1. 优先尝试直播流接口
          try {
            const sources = await liveStream(liveId);
            if (sources.length > 0) {
              list = sources.map((source: JxxkLiveStreamSource, index) => ({
                key: source.label || `live-${index}`,
                name: source.label || `直播流 ${index + 1}`,
                playUrl: source.url,
              }));
            }
          } catch {
            // liveStream 出错时尝试从 playUrls 兜底
          }

          // 2. 兜底：录播主机在课堂直播进行中时，也会实时生成各画面的 HLS 直播切片（playUrls）
          if (list.length === 0) {
            try {
              const screens = await playUrls(liveId);
              list = screens
                .filter((screen: JxxkPlayUrlScreen) => Boolean(screen.playUrl))
                .map((screen) => ({
                  key: screen.streamNo || screen.streamLabel || screen.streamName || "main",
                  name: screen.streamName || "画面",
                  playUrl: screen.playUrl,
                }));
            } catch {
              // 忽略兜底失败
            }
          }

          if (cancelled) return;
          if (list.length === 0) {
            setError("当前暂未检测到可用直播信号，请稍后刷新重试");
            return;
          }
          setAngles(list);
          const last = loadLastStreams().filter((k) => list.some((a) => a.key === k));
          const fallback = list.filter((a) => a.key !== "0");
          const initial =
            last.length > 0
              ? last
              : fallback.length > 0
                ? fallback.map((a) => a.key)
                : list.slice(0, 1).map((a) => a.key);
          setVisible(initial.length > 0 ? initial : list.slice(0, 1).map((a) => a.key));

          // 尽最大努力尝试加载录像元数据（非阻塞，失败静默忽略）
          void lessonStreams(liveId)
            .then((info) => {
              if (!cancelled) setStreams(info);
            })
            .catch(() => {
              // 直播尚未结束时无录像属正常情况，静默跳过
            });
        } else {
          const [info, screens] = await Promise.all([lessonStreams(liveId), playUrls(liveId)]);
          if (cancelled) return;
          setStreams(info);
          const list = screens
            .filter((screen: JxxkPlayUrlScreen) => Boolean(screen.playUrl))
            .map((screen) => ({
              key: screen.streamNo || screen.streamLabel || screen.streamName || "main",
              name: screen.streamName || "画面",
              playUrl: screen.playUrl,
            }));
          setAngles(list);
          // 默认显示集合：上一次的码流选择 ∩ 当前可用；无记录时用三路实拍。
          const last = loadLastStreams().filter((k) => list.some((a) => a.key === k));
          const fallback = list.filter((a) => a.key !== "0");
          const initial = last.length > 0 ? last : fallback.map((a) => a.key);
          setVisible(initial.length > 0 ? initial : list.slice(0, 1).map((a) => a.key));
        }
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [liveId, isLive, retryToken]);

  // 字幕：平台生成的 WebVTT（protect 接口），转 blob URL 交给主播放器。
  useEffect(() => {
    if (isLive || !streams?.mediaVideoId) return;
    let cancelled = false;
    void fetchSubtitle(streams.mediaVideoId)
      .then((vtt) => {
        if (cancelled || !vtt.startsWith("WEBVTT")) return;
        subtitleTextRef.current = vtt;
        const blob = new Blob([vtt], { type: "text/vtt" });
        const url = URL.createObjectURL(blob);
        subtitleBlobRef.current = url;
        setSubtitleUrl(url);
      })
      .catch(() => {
        // 无字幕属正常情况，静默跳过。
      });
    return () => {
      cancelled = true;
      if (subtitleBlobRef.current) {
        URL.revokeObjectURL(subtitleBlobRef.current);
        subtitleBlobRef.current = null;
      }
      setSubtitleUrl(null);
    };
  }, [streams?.mediaVideoId, isLive]);

  const visibleAngles = useMemo(
    () =>
      visible.map((key) => angles.find((a) => a.key === key)).filter((a): a is Angle => Boolean(a)),
    [angles, visible],
  );

  // 按可见集合与顺序创建/销毁 ArtPlayer 实例；新实例经同步插件向已有实例
  // 看齐（不把其它实例拉回 0），因此 promote 触发的重建也无缝衔接进度。
  useEffect(() => {
    if (loading || error) return;
    const groupId = syncGroupId ?? "default";
    const keys = visibleAngles.map((a) => a.key);
    for (const angle of visibleAngles) {
      const key = angle.key;
      if (artRefs.current.has(key)) continue;
      const container = containerRefs.current.get(key);
      if (!container) continue;
      const settings = loadPlayerSettings();
      const art = createPlayer(container, angle, {
        syncGroupId: groupId,
        subtitleUrl,
        // 直播加入必须先静音；回放按持久化默认（其余画面始终静音）。
        muted: isLive || visibleAngles[0]?.key !== key || settings.muted,
        rate: settings.rate,
        isLive,
        onError: (message) => setPlayerError(message),
      });
      artRefs.current.set(key, art);
      // 主画面（有声实例）的静音/倍速变化写入持久化设置。
      if (visibleAngles[0]?.key === key) {
        const video = art.video as HTMLVideoElement | undefined;
        video?.addEventListener("volumechange", () => {
          savePlayerSettings({ muted: video.muted, rate: video.playbackRate });
        });
      }
    }
    // 销毁不再可见的实例
    for (const [key, art] of artRefs.current) {
      if (!keys.includes(key)) {
        art.destroy(false);
        artRefs.current.delete(key);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, loading, error, subtitleUrl, isLive, syncGroupId, angles]);

  // 页面卸载：销毁全部实例。
  useEffect(() => {
    const refs = artRefs.current;
    return () => {
      for (const art of refs.values()) art.destroy(false);
      refs.clear();
    };
  }, []);

  const toggleChecked = (key: string, checked: boolean) => {
    setVisible((prev) => {
      let next: string[];
      if (!checked) {
        if (prev.length === 1) {
          toast.info("至少保留一个画面");
          return prev;
        }
        next = prev.filter((k) => k !== key);
      } else {
        next = prev.includes(key) ? prev : [...prev, key];
      }
      saveLastStreams(next);
      return next;
    });
  };

  const startDownload = useCallback(
    async (angle: Angle) => {
      const stream = streams?.streams.find(
        (s) => (s.streamNo || s.streamLabel || s.name) === angle.key,
      );
      // 文件按「视频源角度.mp4」命名，归入选集目录。
      const fileName = `${angle.name}.mp4`;
      try {
        // 后台任务化：立即返回任务 id，进度在下载管理页与本页同步展示。
        await invokeDownload({
          fileUrl: angle.playUrl,
          fileName,
          semesterName: semesterName ?? "",
          courseName: course ?? "巡课录像",
          teacherName: teacher ?? "",
          lessonName: title ?? angle.name,
          streamName: angle.name,
          streamNo: stream?.streamNo ?? null,
          mediaVideoId: streams?.mediaVideoId ?? "",
          expectedSize: stream?.fileSize ?? null,
          subtitleVtt: subtitleTextRef.current,
        });
        toast.success("已加入下载", {
          description: `${angle.name} · 可在右上角下载管理中查看进度`,
        });
      } catch (err) {
        toast.danger("下载失败", { description: String(err) });
      }
    },
    [streams, title, course, semesterName, teacher],
  );

  const confirmAndDownload = useCallback(
    async (streamName: string, streamNo: string | null) => {
      if (!streams?.mediaVideoId) {
        toast.warning("该课次还没有可下载的录像");
        return;
      }
      try {
        const urls = await downloadUrls(streams.mediaVideoId);
        const target =
          urls.find((u) => u.streamNo === streamNo) ??
          urls.find((u) => u.streamName === streamName);
        if (!target) {
          toast.warning("未找到该画面的下载地址");
          return;
        }
        await startDownload({
          key: target.streamNo || target.streamName || streamName,
          name: streamName,
          playUrl: target.fileUrl,
        });
      } catch (err) {
        toast.danger("获取下载地址失败", { description: String(err) });
      }
    },
    [streams, startDownload],
  );

  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar
          title={title || (isLive ? "课堂直播" : "课堂回放")}
          description={`${course ?? ""}${course ? " · " : ""}${
            isLive ? "直播中" : streams ? `时长 ${formatDuration(streams.duration)}` : "回放"
          }`}
          actions={
            <button
              type="button"
              data-webview-navigation
              className="top-app-bar-icon-button top-app-bar-downloads"
              aria-label="打开录像下载管理"
              title="录像下载管理"
              onClick={() => setManagerOpen(true)}
            >
              <LuDownload aria-hidden="true" size={20} />
            </button>
          }
        />

        {loading ? (
          <div className="py-16 text-center text-sm text-muted flex items-center justify-center gap-2">
            <LoadingIndicator size={16} /> 正在获取播放地址…
          </div>
        ) : error ? (
          <Card variant="outlined" className="p-4">
            <div className="flex items-start justify-between gap-3">
              <div className="text-sm text-foreground">
                <div className="font-medium mb-1">{isLive ? "直播暂不可用" : "回放暂不可用"}</div>
                <div className="text-xs text-muted break-all">{error}</div>
              </div>
              <Button variant="text" onClick={() => setRetryToken((t) => t + 1)} aria-label="重试">
                <LuRotateCw aria-hidden="true" /> 重试
              </Button>
            </div>
          </Card>
        ) : angles.length === 0 ? (
          <div className="py-16 text-center text-sm text-muted">没有可用的画面</div>
        ) : (
          <>
            {playerError && (
              <Banner status="warning" className="mb-2">
                <Banner.Title>视频播放出错</Banner.Title>
                <Banner.Description>{playerError}</Banner.Description>
              </Banner>
            )}
            {/* 全部可见画面等大展示 */}
            <div
              className="grid gap-2"
              style={{
                gridTemplateColumns: `repeat(${Math.min(visibleAngles.length, 2)}, minmax(0, 1fr))`,
              }}
              data-testid="patrol-player"
            >
              {visibleAngles.map((angle) => (
                <div
                  key={angle.key}
                  className="relative overflow-hidden rounded-2xl bg-surface-container"
                >
                  <div
                    ref={(el) => {
                      if (el) containerRefs.current.set(angle.key, el);
                      else containerRefs.current.delete(angle.key);
                    }}
                    className="w-full aspect-video"
                  />
                  <span className="absolute left-2 top-2 z-10 rounded bg-black/60 px-1.5 py-0.5 text-[10px] text-white pointer-events-none">
                    {angle.name}
                    {visibleAngles[0]?.key === angle.key && " 🔊"}
                  </span>
                </div>
              ))}
            </div>

            {/* m3 工具栏：多选 Checkbox 选择当前显示哪些码流画面 */}
            <Toolbar variant="docked" className="mt-3" aria-label="画面选择">
              {angles.map((angle) => (
                <label
                  key={angle.key}
                  className="flex cursor-pointer select-none items-center gap-1.5"
                >
                  <Checkbox
                    checked={visible.includes(angle.key)}
                    onChange={(e: CustomEvent<{ checked: boolean }>) => {
                      toggleChecked(
                        angle.key,
                        e.detail?.checked ?? (e.target as HTMLInputElement).checked,
                      );
                    }}
                    aria-label={`${visible.includes(angle.key) ? "隐藏" : "显示"}${angle.name}`}
                  />
                  <span className="flex items-center gap-1 text-xs text-foreground">
                    {streamIcon(angle.name)}
                    {angle.name}
                  </span>
                </label>
              ))}
            </Toolbar>

            {streams && (
              <section aria-labelledby="patrol-download-heading" className="mt-8">
                <div className="flex items-center gap-2 mb-3">
                  <h2
                    id="patrol-download-heading"
                    className="text-lg font-medium flex items-center gap-2"
                  >
                    <LuDownload className="text-primary" aria-hidden="true" />
                    <span>下载录像</span>
                  </h2>
                </div>
                {!streams.download && (
                  <div className="text-xs text-muted mb-2">平台未开放该课次的下载</div>
                )}
                <div className="flex flex-col gap-2.5">
                  {streams.streams.map((stream) => {
                    // 任务匹配键：课次名 + 画面名
                    const task = downloads.find(
                      (d) => d.lessonName === (title ?? "") && d.streamName === stream.name,
                    );
                    const total = stream.fileSize ?? 0;
                    const percent =
                      task && total > 0
                        ? Math.min(100, Math.max(0, Math.round((task.downloaded / total) * 100)))
                        : 0;
                    const busy = task
                      ? task.status === "downloading" ||
                        task.status === "preparing" ||
                        task.status === "saving" ||
                        task.status === "paused"
                      : false;
                    return (
                      <Card key={stream.id} variant="outlined" className="p-3">
                        <div className="flex items-center justify-between gap-3">
                          <div className="min-w-0">
                            <div className="text-sm font-medium text-foreground">{stream.name}</div>
                            <div className="text-xs text-muted">
                              {formatBytes(stream.fileSize)} · {formatDuration(stream.duration)}
                            </div>
                          </div>
                          <Button
                            variant="text"
                            disabled={!streams.download || busy}
                            onClick={() =>
                              setConfirmStream({
                                name: stream.name,
                                streamNo: stream.streamNo ?? null,
                              })
                            }
                            aria-label={`下载${stream.name}`}
                          >
                            <LuDownload aria-hidden="true" />
                            {task?.status === "completed"
                              ? "已完成"
                              : task?.status === "failed"
                                ? "重试"
                                : "下载"}
                          </Button>
                        </div>
                        {task &&
                          (task.status === "downloading" ||
                            task.status === "paused" ||
                            task.status === "preparing" ||
                            task.status === "saving") && (
                            <div className="mt-2 flex items-center gap-2">
                              <Progress
                                variant="linear"
                                value={percent}
                                max={100}
                                className="w-full"
                                aria-label={`${stream.name} 下载进度`}
                              />
                              <span className="text-xs text-muted whitespace-nowrap">
                                {percent}% · {formatBytes(task.downloaded)}/
                                {formatBytes(task.total)}
                                {task.status === "downloading" && ` · ${formatBytes(task.speed)}/s`}
                              </span>
                            </div>
                          )}
                        {task?.status === "failed" && task.error && (
                          <div className="text-xs text-danger mt-1 break-all">{task.error}</div>
                        )}
                      </Card>
                    );
                  })}
                </div>
              </section>
            )}
          </>
        )}

        {confirmStream &&
          createPortal(
            <Dialog
              open
              onClose={() => setConfirmStream(null)}
              headline="确认下载"
              size="small"
              actions={
                <>
                  <Button variant="text" onClick={() => setConfirmStream(null)}>
                    取消
                  </Button>
                  <Button
                    variant="filled"
                    onClick={() => {
                      const target = confirmStream;
                      setConfirmStream(null);
                      void confirmAndDownload(target.name, target.streamNo);
                    }}
                  >
                    开始下载
                  </Button>
                </>
              }
            >
              <p>
                即将下载「{confirmStream.name}」录像{course ? `（${course}）` : ""}
                ，文件较大（
                {formatBytes(
                  streams?.streams.find((s) => s.name === confirmStream.name)?.fileSize ?? 0,
                )}
                ），将通过 aria2 后台下载到 {downloadDir ?? "应用保存目录"}，支持断点续传。
              </p>
            </Dialog>,
            document.body,
          )}
      </PageSurface>

      {/* 下载管理覆盖层：不导航，播放实例保持挂载、进度继续 */}
      {managerOpen &&
        createPortal(
          <div className="fixed inset-0 z-50 flex flex-col bg-surface">
            <TopAppBar
              title="录像下载管理"
              description="巡课录像下载任务：进度、暂停与删除"
              backLabel="返回播放"
              onBack={() => setManagerOpen(false)}
            />
            <div className="flex-1 overflow-y-auto">
              <DownloadManagerView />
            </div>
          </div>,
          document.body,
        )}
    </AuthGate>
  );
}
