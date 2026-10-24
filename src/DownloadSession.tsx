import { useMemo, useState } from "react";
import { Chip, ProgressBar } from "@heroui/react";
import { LuChevronDown, LuChevronUp, LuLayers } from "react-icons/lu";

export interface DownloadSessionData {
  downloadedBytes: number;
  totalBytes: number;
  speedBytesPerSec?: number;
  port?: number;
  numPieces?: number;
  pieceLength?: number;
  bitfield?: string | null;
  connections?: number;
  status?: string;
  error?: string | null;
}

export function formatDownloadSpeed(bytesPerSec?: number): string {
  if (!bytesPerSec || bytesPerSec <= 0) return "0 B/s";
  if (bytesPerSec < 1024) return `${bytesPerSec} B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

export function formatBytes(bytes?: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export const formatModelBytes = formatBytes;

/**
 * 解析 aria2 十六进制 bitfield 字符串为各分片的完成状态布尔数组
 * @param bitfield 十六进制位图，最高位对应第 0 个分片，1 为已下载完成，0 为未完成
 * @param numPieces 分片总数
 */
export function parseAria2Bitfield(bitfield?: string | null, numPieces?: number): boolean[] {
  if (!bitfield || !numPieces || numPieces <= 0) return [];
  const result: boolean[] = [];

  for (let i = 0; i < bitfield.length && result.length < numPieces; i++) {
    const val = parseInt(bitfield[i], 16);
    if (Number.isNaN(val)) continue;
    for (let bit = 3; bit >= 0 && result.length < numPieces; bit--) {
      result.push((val & (1 << bit)) !== 0);
    }
  }

  // 若 bitfield 长度不足，以 false 补齐至 numPieces
  while (result.length < numPieces) {
    result.push(false);
  }

  return result;
}

export interface DownloadSessionProps {
  title?: string;
  session: DownloadSessionData;
  className?: string;
  defaultExpanded?: boolean;
}

export function DownloadSession({
  title = "正在通过 aria2c 下载...",
  session,
  className = "",
  defaultExpanded = false,
}: DownloadSessionProps) {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);

  const percent =
    session.totalBytes > 0
      ? Math.min(100, Math.max(0, Math.round((session.downloadedBytes / session.totalBytes) * 100)))
      : 0;

  const pieces = useMemo(() => {
    return parseAria2Bitfield(session.bitfield, session.numPieces);
  }, [session.bitfield, session.numPieces]);

  const completedPiecesCount = useMemo(() => {
    return pieces.filter(Boolean).length;
  }, [pieces]);

  const hasPieceDetails = Boolean(session.numPieces && session.numPieces > 0);

  return (
    <div
      data-testid="aria2-download-session"
      className={`grid w-full min-w-0 max-w-full gap-3 rounded-2xl border border-separator/60 bg-surface/90 p-3.5 shadow-xs backdrop-blur-sm ${className}`}
    >
      {/* Session 标头：活动指示点 + 会话标题 + 端口 / 线程标签 */}
      <div className="flex flex-wrap items-center justify-between gap-2 min-w-0">
        <div className="flex items-center gap-2 min-w-0 overflow-hidden">
          <span className="relative flex size-2 shrink-0" aria-hidden="true">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-75" />
            <span className="relative inline-flex size-2 rounded-full bg-primary" />
          </span>
          <span className="font-mono text-[11px] font-semibold uppercase tracking-wider text-primary shrink-0">
            aria2
          </span>
          <span className="truncate text-xs text-muted" title={title}>
            {title}
          </span>
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          {session.port != null && session.port > 0 && (
            <Chip
              size="sm"
              variant="soft"
              color="accent"
              className="font-mono text-[11px] font-semibold"
            >
              RPC :{session.port}
            </Chip>
          )}
          {session.connections != null && session.connections > 0 && (
            <Chip size="sm" variant="soft" className="font-mono text-[11px]">
              {session.connections} 线程
            </Chip>
          )}
        </div>
      </div>

      {/* 主进度条 */}
      <div className="space-y-1.5 min-w-0">
        <div className="flex items-center justify-between text-xs min-w-0 gap-2">
          <span className="font-mono text-muted truncate">
            {formatBytes(session.downloadedBytes)} / {formatBytes(session.totalBytes)}
          </span>
          <span className="font-mono font-medium text-foreground shrink-0">{percent}%</span>
        </div>
        <ProgressBar
          aria-label={title}
          value={percent}
          minValue={0}
          maxValue={100}
          className="w-full"
        >
          <ProgressBar.Track>
            <ProgressBar.Fill />
          </ProgressBar.Track>
        </ProgressBar>
        <div className="flex items-center justify-between font-mono text-xs text-muted gap-2 min-w-0">
          <span className="text-[11px]">
            {session.numPieces ? `分片数: ${session.numPieces}` : "准备中"}
          </span>
          <span className="font-medium text-foreground shrink-0">
            {formatDownloadSpeed(session.speedBytesPerSec)}
          </span>
        </div>
      </div>

      {/* 分片下载进度折叠展开模块 */}
      {hasPieceDetails && (
        <div className="rounded-xl border border-separator/40 bg-surface-secondary/40 p-2.5 space-y-2 min-w-0">
          <button
            type="button"
            onClick={() => setIsExpanded((prev) => !prev)}
            className="flex w-full items-center justify-between text-left text-xs text-muted hover:text-foreground transition-colors cursor-pointer select-none"
            aria-expanded={isExpanded}
          >
            <span className="flex items-center gap-1.5 font-mono text-[11px] truncate">
              <LuLayers className="size-3.5 shrink-0" aria-hidden="true" />
              分片进度: {completedPiecesCount} / {session.numPieces} 完成
              {session.pieceLength ? ` · 单片 ${formatBytes(session.pieceLength)}` : ""}
            </span>
            <span className="shrink-0 flex items-center text-muted">
              {isExpanded ? (
                <LuChevronUp className="size-4" aria-hidden="true" />
              ) : (
                <LuChevronDown className="size-4" aria-hidden="true" />
              )}
            </span>
          </button>

          {isExpanded && (
            <div className="space-y-2 pt-1 animate-in fade-in-50 duration-200">
              {/* 分片色块矩阵 (Piece Block Grid) */}
              <div
                className="grid gap-1 p-2 rounded-lg bg-surface border border-separator/30 max-h-36 overflow-y-auto"
                style={{
                  gridTemplateColumns: `repeat(auto-fill, minmax(${pieces.length > 64 ? "8px" : "12px"}, 1fr))`,
                }}
                role="region"
                aria-label="分片下载状态矩阵"
              >
                {pieces.map((isDone, idx) => (
                  <div
                    key={`piece-${idx}`}
                    title={`分片 #${idx + 1}: ${isDone ? "已完成" : "下载中/待分配"}`}
                    className={`h-2.5 rounded-xs transition-colors duration-150 ${
                      isDone
                        ? "bg-primary shadow-xs"
                        : "bg-surface-secondary/70 border border-separator/40"
                    }`}
                  />
                ))}
              </div>
              <div className="flex items-center justify-between text-[10px] font-mono text-muted">
                <span className="flex items-center gap-1">
                  <span className="size-2 rounded-xs bg-primary inline-block" /> 已完成
                </span>
                <span className="flex items-center gap-1">
                  <span className="size-2 rounded-xs bg-surface-secondary border border-separator/40 inline-block" /> 下载中 / 未完成
                </span>
                <span>
                  {session.numPieces
                    ? `${((completedPiecesCount / session.numPieces) * 100).toFixed(1)}%`
                    : "0%"}
                </span>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * 兼容旧组件的适配封装，确保现有调用与测试无损过渡
 */
export function ModelProgressBar({
  downloadedBytes,
  totalBytes,
  speedBytesPerSec,
  label = "正在下载模型...",
  port,
  numPieces,
  pieceLength,
  bitfield,
  connections,
}: {
  downloadedBytes: number;
  totalBytes: number;
  speedBytesPerSec: number;
  label?: string;
  port?: number;
  numPieces?: number;
  pieceLength?: number;
  bitfield?: string | null;
  connections?: number;
}) {
  return (
    <DownloadSession
      title={label}
      session={{
        downloadedBytes,
        totalBytes,
        speedBytesPerSec,
        port,
        numPieces,
        pieceLength,
        bitfield,
        connections,
      }}
    />
  );
}
