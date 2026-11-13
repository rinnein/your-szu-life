import {
  useEffect,
  useState,
  type CSSProperties,
  type HTMLAttributeReferrerPolicy,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * 前端资源代理策略：
 * - `announcement`：公告/更新日志等可信内容源，由 Rust 侧校验是否为当前公告图片；
 * - `image`：任意第三方公网图片（题库、贡献者头像等），由 Rust 侧拒绝内网地址。
 */
export type RemoteResourceKind = "announcement" | "image";

/**
 * 从首部字节识别图片类型。Rust 侧可能返回原始（未重编码）图片，
 * 因此这里必须覆盖题库契约允许的 PNG/JPEG/GIF/WebP/BMP/AVIF。
 */
export function imageMimeType(bytes: ArrayBuffer): string | undefined {
  if (bytes.byteLength < 12) return undefined;
  const head = new Uint8Array(bytes, 0, 12);
  const startsWith = (...signature: number[]) =>
    signature.every((value, index) => head[index] === value);
  if (startsWith(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)) return "image/png";
  if (startsWith(0xff, 0xd8, 0xff)) return "image/jpeg";
  if (startsWith(0x47, 0x49, 0x46, 0x38)) return "image/gif";
  if (startsWith(0x42, 0x4d)) return "image/bmp";
  const isWebp =
    startsWith(0x52, 0x49, 0x46, 0x46) &&
    head[8] === 0x57 &&
    head[9] === 0x45 &&
    head[10] === 0x42 &&
    head[11] === 0x50;
  if (isWebp) return "image/webp";
  const brand = String.fromCharCode(head[4], head[5], head[6], head[7]);
  const subtype = String.fromCharCode(head[8], head[9], head[10], head[11]);
  if (brand === "ftyp" && (subtype === "avif" || subtype === "avis")) return "image/avif";
  return undefined;
}

type CachedImage = {
  promise: Promise<string>;
  refs: number;
  resolved?: string;
};

const objectUrls = new Map<string, CachedImage>();

function cacheKey(url: string, kind: RemoteResourceKind): string {
  return `${kind}\u0000${url}`;
}

function acquire(url: string, kind: RemoteResourceKind): Promise<string> {
  const key = cacheKey(url, kind);
  const existing = objectUrls.get(key);
  if (existing) {
    existing.refs += 1;
    return existing.promise;
  }
  const entry: CachedImage = { promise: Promise.resolve(""), refs: 1 };
  entry.promise = invoke<ArrayBuffer>("load_remote_resource", { url, kind })
    .then((bytes) => {
      if (!(bytes instanceof ArrayBuffer)) {
        throw new Error("Invalid native resource response");
      }
      const type = imageMimeType(bytes);
      if (!type) throw new Error("Unsupported resource type");
      const src = URL.createObjectURL(new Blob([bytes], { type }));
      entry.resolved = src;
      return src;
    })
    .catch((error: unknown) => {
      objectUrls.delete(key);
      throw error;
    });
  objectUrls.set(key, entry);
  return entry.promise;
}

function release(key: string): void {
  const existing = objectUrls.get(key);
  if (!existing) return;
  existing.refs -= 1;
  if (existing.refs > 0) return;
  objectUrls.delete(key);
  if (existing.resolved) {
    URL.revokeObjectURL(existing.resolved);
    return;
  }
  // 请求可能尚未完成；完成后再撤销，避免泄漏 Blob URL。
  void existing.promise.then((src) => URL.revokeObjectURL(src)).catch(() => undefined);
}

/**
 * 通过 Rust 侧资源代理加载图片，返回可给 `<img src>` 使用的 Blob URL。
 *
 * 相同 URL + 策略的并发/重复挂载会共享同一次请求与同一个 Blob URL，
 * 最后一个使用者卸载时释放。
 */
export function useRemoteImage(
  url: string | null | undefined,
  kind: RemoteResourceKind = "image",
): { src?: string; failed: boolean; loading: boolean } {
  const trimmed = url?.trim() ?? "";
  const key = trimmed ? cacheKey(trimmed, kind) : "";
  const [state, setState] = useState<{ key: string; src?: string; failed?: boolean }>({ key: "" });

  useEffect(() => {
    if (!trimmed) return;
    let active = true;
    acquire(trimmed, kind)
      .then((src) => {
        if (active) setState({ key, src });
      })
      .catch(() => {
        if (active) setState({ key, failed: true });
      });
    return () => {
      active = false;
      release(key);
    };
  }, [kind, key, trimmed]);

  const current = state.key === key ? state : undefined;
  return {
    src: current?.src,
    failed: Boolean(current?.failed),
    loading: Boolean(key) && !current,
  };
}

/**
 * 远程图片的统一下载出口。data:/blob: 直接渲染，http(s) 一律经过 Rust 代理。
 */
export function RemoteImage({
  src,
  kind = "image",
  alt = "",
  className,
  style,
  width,
  height,
  fallback,
  loading,
  referrerPolicy,
}: {
  src?: string | null;
  kind?: RemoteResourceKind;
  alt?: string;
  className?: string;
  style?: CSSProperties;
  width?: number | string;
  height?: number | string;
  fallback?: ReactNode;
  loading?: "lazy" | "eager";
  referrerPolicy?: HTMLAttributeReferrerPolicy;
}): ReactNode {
  const raw = src?.trim() ?? "";
  if (!raw) return null;
  if (raw.startsWith("data:") || raw.startsWith("blob:")) {
    return (
      <img
        src={raw}
        alt={alt}
        className={className}
        style={style}
        width={width}
        height={height}
        loading={loading}
        referrerPolicy={referrerPolicy}
      />
    );
  }
  return (
    <ProxiedImage
      raw={raw}
      kind={kind}
      alt={alt}
      className={className}
      style={style}
      width={width}
      height={height}
      fallback={fallback}
      loading={loading}
    />
  );
}

function ProxiedImage({
  raw,
  kind,
  alt,
  className,
  style,
  width,
  height,
  fallback,
  loading,
}: {
  raw: string;
  kind: RemoteResourceKind;
  alt: string;
  className?: string;
  style?: CSSProperties;
  width?: number | string;
  height?: number | string;
  fallback?: ReactNode;
  loading?: "lazy" | "eager";
}): ReactNode {
  const { src, failed } = useRemoteImage(raw, kind);
  if (failed) {
    if (fallback !== undefined) return fallback;
    return alt ? <span className={className}>{alt}</span> : null;
  }
  if (!src) return null;
  return (
    <img
      src={src}
      alt={alt}
      className={className}
      style={style}
      width={width}
      height={height}
      loading={loading}
    />
  );
}
