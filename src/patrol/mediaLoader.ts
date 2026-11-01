import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  HlsConfig,
  Loader,
  LoaderCallbacks,
  LoaderConfiguration,
  LoaderContext,
  LoaderResponse,
  LoaderStats,
} from "hls.js";

/**
 * hls.js 自定义 loader：`jxxk-media:<base64url(目标)>` 形态的地址经 Tauri
 * 命令（jxxk_fetch_media）回源拉取，媒体字节 base64 后分片经 Channel 推送。
 * 大载荷在 postMessage 模式的 IPC 下无法以原始字节投递（会退化为数字数组、
 * 或需要走可能被拦截的 ipc:// fetch），分片 JSON 直发是两种模式都可靠的路径。
 */
export const MEDIA_SCHEME = "jxxk-media:";

/** 把巡课域的绝对 https URL 编码为媒体地址。 */
export function toMediaUrl(target: string): string {
  const bytes = new TextEncoder().encode(target);
  let binary = "";
  bytes.forEach((b) => {
    binary += String.fromCharCode(b);
  });
  return MEDIA_SCHEME + btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function base64ToArrayBuffer(encoded: string): ArrayBuffer {
  const binary = atob(encoded);
  return Uint8Array.from(binary, (c) => c.charCodeAt(0)).buffer;
}

export class MediaLoader implements Loader<LoaderContext> {
  private aborted = false;
  /** 最近一次 loader 层错误文本：hls.js 的 ErrorData 不一定携带，供页面展示。 */
  public static lastError: string | null = null;
  /** hls.js Loader 接口要求公开 context 与 stats（重试/统计读取）。 */
  public context: LoaderContext | null = null;
  public stats: LoaderStats = {
    aborted: false,
    loaded: 0,
    retry: 0,
    total: 0,
    chunkCount: 0,
    bwEstimate: 0,
    loading: { start: 0, first: 0, end: 0 },
    parsing: { start: 0, end: 0 },
    buffering: { start: 0, first: 0, end: 0 },
  };
  private callbacks: LoaderCallbacks<LoaderContext> | null = null;

  constructor(protected hlsConfig: HlsConfig) {}

  load(
    context: LoaderContext,
    _config: LoaderConfiguration,
    callbacks: LoaderCallbacks<LoaderContext>,
  ) {
    this.context = context;
    this.callbacks = callbacks;
    const url = context.url;
    if (!url.startsWith(MEDIA_SCHEME)) {
      const text = `不支持的媒体地址: ${url}`;
      MediaLoader.lastError = text;
      this.callbacks.onError({ code: 0, text }, context, null, this.stats);
      return;
    }
    const trequest = performance.now();
    this.stats = {
      aborted: false,
      loaded: 0,
      retry: 0,
      total: 0,
      chunkCount: 0,
      bwEstimate: 0,
      loading: { start: trequest, first: 0, end: 0 },
      parsing: { start: 0, end: 0 },
      buffering: { start: 0, first: 0, end: 0 },
    };
    // 后端把媒体字节 base64 后分片经 Channel 推送（postMessage 模式下也可靠）。
    const chunks: string[] = [];
    const channel = new Channel<string>();
    channel.onmessage = (chunk) => {
      if (this.aborted) return;
      chunks.push(chunk);
    };
    void invoke("jxxk_fetch_media", { url, onData: channel })
      .then(() => {
        if (this.aborted) return;
        const now = performance.now();
        const bytes = new Uint8Array(base64ToArrayBuffer(chunks.join("")));
        const loaded = bytes.byteLength;
        if (loaded === 0) {
          MediaLoader.lastError = "媒体数据为空";
          this.callbacks?.onError({ code: 0, text: "媒体数据为空" }, context, null, this.stats);
          return;
        }
        // 播放列表必须以字符串交付（hls.js 的文本分支对 ArrayBuffer 会崩），
        // 分片等二进制上下文交付 Uint8Array。
        // 播放列表（manifest/level/steering）为文本；其余为二进制。
        const isPlaylist =
          context.type === "manifest" ||
          context.type === "level" ||
          context.type === "steering-manifest";
        const data: LoaderResponse["data"] = isPlaylist ? new TextDecoder().decode(bytes) : bytes;
        const stats = this.stats;
        stats.loaded = loaded;
        stats.total = loaded;
        stats.chunkCount = 1;
        stats.loading.first = now;
        stats.loading.end = now;
        const response: LoaderResponse = { url, data };
        this.callbacks?.onSuccess(response, stats, context, null);
      })
      .catch((error: unknown) => {
        if (this.aborted) return;
        MediaLoader.lastError = String(error);
        this.callbacks?.onError({ code: 0, text: String(error) }, context, null, this.stats);
      });
  }

  abort() {
    // 已在途的 invoke 无法取消，abort 标记使结果被丢弃。
    this.aborted = true;
  }

  destroy() {
    this.aborted = true;
    this.context = null;
    this.callbacks = null;
  }
}

/** 让 hls.js 使用 MediaLoader 的配置片段。 */
export function mediaLoaderConfig(): Partial<HlsConfig> {
  return { loader: MediaLoader };
}
