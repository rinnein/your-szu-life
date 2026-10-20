import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Accordion,
  Alert,
  Button,
  Chip,
  Pagination,
  SearchField,
  Spinner,
  Table,
  toast,
} from "@heroui/react";
import {
  LuChevronDown,
  LuChevronUp,
  LuCopy,
  LuEraser,
  LuListCollapse,
  LuPower,
  LuRefreshCw,
  LuTable,
} from "react-icons/lu";
import { Card, ModuleHeader, PageShell } from "./components";
import { copyText } from "./clipboard";

type DebugMode = { enabled: boolean; logCount: number };
type DebugLog = {
  timestampMs: number;
  level: string;
  source: string;
  stage: string;
  connected?: boolean | null;
  transport?: string | null;
  capabilitiesAvailable?: boolean | null;
  wifiPermission?: string | null;
  locationEnabled?: boolean | null;
  ssid?: string | null;
  ip?: string | null;
  portal?: string | null;
  httpStatus?: number | null;
  elapsedMs?: number | null;
  errorKind?: string | null;
  message?: string | null;
};

type DebugLogChunk = {
  lines: string[];
  total: number;
  hasMore: boolean;
};

const initialMode: DebugMode = { enabled: false, logCount: 0 };
const PAGE_SIZE_OPTIONS = [20, 50, 100] as const;
const CHUNK_SIZE = 100;

function getLevelColor(level: string): "default" | "success" | "warning" | "danger" | "accent" {
  switch (level.toLowerCase()) {
    case "error":
      return "danger";
    case "warn":
    case "warning":
      return "warning";
    case "info":
      return "accent";
    default:
      return "default";
  }
}

function getLogSummary(log: DebugLog) {
  const network = [
    log.connected == null ? null : log.connected ? "已连接" : "未连接",
    log.transport,
    log.ssid ? `SSID=${log.ssid}` : null,
    log.ip ? `IP=${log.ip}` : null,
    log.capabilitiesAvailable == null
      ? null
      : `能力=${log.capabilitiesAvailable ? "可用" : "不可用"}`,
    log.wifiPermission ? `Wi-Fi权限=${log.wifiPermission}` : null,
    log.locationEnabled == null ? null : `位置=${log.locationEnabled ? "开" : "关"}`,
  ]
    .filter(Boolean)
    .join(" ");
  const portal = log.portal ? `门户=${log.portal}` : "";
  const http = log.httpStatus == null ? "" : `HTTP=${log.httpStatus}`;
  const elapsed = log.elapsedMs == null ? "" : `${log.elapsedMs}ms`;
  const error = log.errorKind ? `错误=${log.errorKind}` : "";
  return [network, portal, http, elapsed, error, log.message].filter(Boolean).join(" ");
}

function formatLog(log: DebugLog) {
  const time = new Date(log.timestampMs).toLocaleString();
  const detail = getLogSummary(log);
  return `[${time}] ${log.level.toUpperCase()} ${log.source}/${log.stage}${detail ? ` - ${detail}` : ""}`;
}

function formatLogs(logs: DebugLog[]) {
  return logs.length ? logs.map(formatLog).join("\n") : "暂无诊断日志";
}

function getLogDetails(log: DebugLog) {
  const fields: Array<{ label: string; value: string | number | boolean | null | undefined }> = [
    {
      label: "时间戳",
      value: `${new Date(log.timestampMs).toLocaleString()} (${log.timestampMs})`,
    },
    { label: "日志级别", value: log.level.toUpperCase() },
    { label: "来源模块", value: log.source },
    { label: "阶段环节", value: log.stage },
    {
      label: "网络连接",
      value: log.connected == null ? null : log.connected ? "已连接" : "未连接",
    },
    { label: "传输媒介", value: log.transport },
    { label: "SSID", value: log.ssid },
    { label: "IP 地址", value: log.ip },
    {
      label: "网络能力",
      value:
        log.capabilitiesAvailable == null ? null : log.capabilitiesAvailable ? "可用" : "不可用",
    },
    { label: "Wi-Fi 权限", value: log.wifiPermission },
    {
      label: "位置服务",
      value: log.locationEnabled == null ? null : log.locationEnabled ? "开启" : "关闭",
    },
    { label: "探测门户", value: log.portal },
    { label: "HTTP 状态", value: log.httpStatus },
    { label: "耗时", value: log.elapsedMs == null ? null : `${log.elapsedMs} ms` },
    { label: "错误类型", value: log.errorKind },
    { label: "消息内容", value: log.message },
  ];
  return fields.filter((f) => f.value != null && f.value !== "");
}

export default function DebugLogPage() {
  const navigation = useLogicalNavigation();
  const [mode, setMode] = useState(initialMode);
  const [logs, setLogs] = useState<DebugLog[]>([]);
  const [busy, setBusy] = useState(true);
  const [loadingChunk, setLoadingChunk] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [totalInFile, setTotalInFile] = useState(0);
  const [message, setMessage] = useState<string>();
  const [search, setSearch] = useState("");
  const [selectedLevel, setSelectedLevel] = useState<string>("ALL");
  const [viewMode, setViewMode] = useState<"table" | "accordion">("table");
  const [expandedKeys, setExpandedKeys] = useState<Set<string | number>>(() => new Set());
  const [expandedRowKeys, setExpandedRowKeys] = useState<Set<string>>(() => new Set());

  // Pagination states
  const [currentPage, setCurrentPage] = useState(1);
  const [pageSize, setPageSize] = useState<number>(50);

  const inFlightFetchRef = useRef<Promise<boolean> | null>(null);
  const loadingChunkRef = useRef(false);
  const hasMoreRef = useRef(false);
  const logsLengthRef = useRef(0);

  useEffect(() => {
    hasMoreRef.current = hasMore;
  }, [hasMore]);

  useEffect(() => {
    logsLengthRef.current = logs.length;
  }, [logs.length]);

  const load = async () => {
    setBusy(true);
    setCurrentPage(1);
    try {
      const nextMode = (await invoke("get_debug_mode")) as DebugMode;
      let nextLogs: DebugLog[] = [];
      let nextTotal = 0;
      let nextHasMore = false;

      if (nextMode.enabled) {
        // Initial chunk loading with JSONL first, fallback to get_debug_logs
        try {
          const chunk = (await invoke("get_debug_logs_jsonl", {
            offset: 0,
            limit: CHUNK_SIZE,
          })) as DebugLogChunk | undefined;

          if (chunk && Array.isArray(chunk.lines)) {
            nextLogs = chunk.lines
              .map((line) => {
                try {
                  return JSON.parse(line) as DebugLog;
                } catch {
                  return null;
                }
              })
              .filter((item): item is DebugLog => item !== null);
            nextTotal = chunk.total;
            nextHasMore = chunk.hasMore;
          } else {
            const all = ((await invoke("get_debug_logs")) as DebugLog[]) ?? [];
            nextLogs = [...all].reverse();
            nextTotal = nextLogs.length;
            nextHasMore = false;
          }
        } catch {
          const all = ((await invoke("get_debug_logs")) as DebugLog[]) ?? [];
          nextLogs = [...all].reverse();
          nextTotal = nextLogs.length;
          nextHasMore = false;
        }
      }

      setMode(nextMode);
      setLogs(nextLogs);
      logsLengthRef.current = nextLogs.length;
      setTotalInFile(nextTotal);
      setHasMore(nextHasMore);
      hasMoreRef.current = nextHasMore;
    } catch (cause) {
      const errorMsg = `无法读取诊断日志：${String(cause)}`;
      setMessage(errorMsg);
      toast.danger(errorMsg);
    } finally {
      setBusy(false);
    }
  };

  // Fetch more chunk on demand with deduplication and dynamic limit
  const fetchNextChunk = async (neededCount?: number): Promise<boolean> => {
    if (inFlightFetchRef.current) {
      return inFlightFetchRef.current;
    }
    if (!hasMoreRef.current) return false;

    const runFetch = async (): Promise<boolean> => {
      setLoadingChunk(true);
      loadingChunkRef.current = true;
      try {
        const currentCount = logsLengthRef.current;
        const requestedLimit =
          neededCount && neededCount > currentCount
            ? Math.max(CHUNK_SIZE, neededCount - currentCount)
            : CHUNK_SIZE;

        const chunk = (await invoke("get_debug_logs_jsonl", {
          offset: currentCount,
          limit: requestedLimit,
        })) as DebugLogChunk;

        const parsed = chunk.lines
          .map((line) => {
            try {
              return JSON.parse(line) as DebugLog;
            } catch {
              return null;
            }
          })
          .filter((item): item is DebugLog => item !== null);

        setLogs((prev) => [...prev, ...parsed]);
        logsLengthRef.current = currentCount + parsed.length;
        setHasMore(chunk.hasMore);
        hasMoreRef.current = chunk.hasMore;
        setTotalInFile(chunk.total);
        return parsed.length > 0;
      } catch (cause) {
        const err = `分片加载失败：${String(cause)}`;
        toast.danger(err);
        return false;
      } finally {
        setLoadingChunk(false);
        loadingChunkRef.current = false;
        inFlightFetchRef.current = null;
      }
    };

    const promise = runFetch();
    inFlightFetchRef.current = promise;
    return promise;
  };

  const handleLoadMoreChunkManual = async () => {
    const success = await fetchNextChunk();
    if (success) {
      toast.success("已载入更多历史日志分片");
    }
  };

  useEffect(() => {
    void load();
    let active = true;
    let unlistenMode: (() => void) | undefined;
    let unlistenLog: (() => void) | undefined;

    void listen<DebugMode>("debug-mode-changed", (event) => {
      if (!active) return;
      setMode(event.payload);
      if (!event.payload.enabled) {
        setLogs([]);
        logsLengthRef.current = 0;
        setHasMore(false);
        hasMoreRef.current = false;
        setTotalInFile(0);
      }
    }).then((stop) => {
      if (active) unlistenMode = stop;
      else stop();
    });

    void listen<DebugLog>("debug-log-added", (event) => {
      if (!active) return;
      setLogs((current) => [event.payload, ...current]);
      logsLengthRef.current += 1;
      setTotalInFile((current) => current + 1);
    }).then((stop) => {
      if (active) unlistenLog = stop;
      else stop();
    });

    return () => {
      active = false;
      unlistenMode?.();
      unlistenLog?.();
    };
  }, []);

  const levelCounts = useMemo(() => {
    const counts: Record<string, number> = { ALL: logs.length, INFO: 0, WARN: 0, ERROR: 0 };
    for (const log of logs) {
      const lvl = log.level.toUpperCase();
      if (lvl === "WARN" || lvl === "WARNING") {
        counts.WARN = (counts.WARN ?? 0) + 1;
      } else if (lvl === "ERROR") {
        counts.ERROR = (counts.ERROR ?? 0) + 1;
      } else if (lvl === "INFO") {
        counts.INFO = (counts.INFO ?? 0) + 1;
      }
    }
    return counts;
  }, [logs]);

  const filteredLogs = useMemo(() => {
    const query = search.trim().toLowerCase();
    return logs.filter((log) => {
      if (selectedLevel !== "ALL") {
        const lvl = log.level.toUpperCase();
        if (selectedLevel === "WARN" && lvl !== "WARN" && lvl !== "WARNING") return false;
        if (selectedLevel === "ERROR" && lvl !== "ERROR") return false;
        if (selectedLevel === "INFO" && lvl !== "INFO") return false;
      }
      if (!query) return true;
      const haystack = [
        new Date(log.timestampMs).toLocaleString(),
        log.level,
        log.source,
        log.stage,
        log.message,
        log.ssid,
        log.ip,
        log.portal,
        log.transport,
        log.errorKind,
        log.httpStatus != null ? String(log.httpStatus) : "",
        formatLog(log),
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  }, [logs, search, selectedLevel]);

  // Pagination calculation
  const totalFiltered = filteredLogs.length;
  // If there are more chunks on disk, provide at least one more page to navigate to
  const loadedPages = Math.max(1, Math.ceil(totalFiltered / pageSize));
  const estimatedTotalPages = hasMore
    ? Math.max(loadedPages + 1, Math.ceil(totalInFile / pageSize))
    : loadedPages;
  const safePage = Math.min(Math.max(1, currentPage), estimatedTotalPages);

  const paginatedLogs = useMemo(() => {
    const start = (safePage - 1) * pageSize;
    return filteredLogs.slice(start, start + pageSize);
  }, [filteredLogs, safePage, pageSize]);

  const startRecordIndex = totalFiltered > 0 ? (safePage - 1) * pageSize + 1 : 0;
  const endRecordIndex = Math.min(safePage * pageSize, totalFiltered);

  // Auto-fetch next chunk when user navigates towards or reaches the last loaded page
  const handlePageChange = async (targetPage: number) => {
    const clampedTarget = Math.max(1, targetPage);
    setCurrentPage(clampedTarget);

    const neededIndex = clampedTarget * pageSize;
    const isFiltered = search.trim() !== "" || selectedLevel !== "ALL";
    const neededFilteredCount = (clampedTarget - 1) * pageSize + 1;

    const needsMoreData = isFiltered
      ? filteredLogs.length < neededFilteredCount && hasMoreRef.current
      : neededIndex > logsLengthRef.current && hasMoreRef.current;

    if (needsMoreData) {
      await fetchNextChunk(neededIndex);
    } else if (hasMoreRef.current && !loadingChunkRef.current) {
      // Auto-prefetch next chunk in background when navigating to or near the last loaded page
      const currentLoadedCount = isFiltered ? filteredLogs.length : logsLengthRef.current;
      const currentLoadedPages = Math.max(1, Math.ceil(currentLoadedCount / pageSize));
      if (clampedTarget >= currentLoadedPages) {
        void fetchNextChunk();
      }
    }
  };

  const handleCopy = async (targetLogs: DebugLog[]) => {
    const text = formatLogs(targetLogs);
    const success = await copyText(text);
    if (success) {
      setMessage("日志已复制");
      toast.success("日志已复制");
    } else {
      setMessage("无法自动复制，请长按日志文本手动复制");
      toast.danger("无法自动复制，请长按日志文本手动复制");
    }
  };

  const handleCopySingle = async (log: DebugLog) => {
    const text = formatLog(log);
    const success = await copyText(text);
    if (success) {
      toast.success("单条日志已复制");
    } else {
      toast.danger("无法自动复制，请长按日志文本手动复制");
    }
  };

  const handleClear = async () => {
    try {
      await invoke("clear_debug_logs");
      setLogs([]);
      logsLengthRef.current = 0;
      setTotalInFile(0);
      setHasMore(false);
      hasMoreRef.current = false;
      setMode((current) => ({ ...current, logCount: 0 }));
      setMessage("日志已清空");
      toast.success("日志已清空");
    } catch (cause) {
      const err = `清空失败：${String(cause)}`;
      setMessage(err);
      toast.danger(err);
    }
  };

  const handleDisable = async () => {
    try {
      await invoke("set_debug_mode", { enabled: false });
      toast.info("调试模式已关闭");
      await navigation.back();
    } catch (cause) {
      const err = `关闭调试模式失败：${String(cause)}`;
      setMessage(err);
      toast.danger(err);
    }
  };

  const handleExpandAll = () => {
    setExpandedKeys(new Set(paginatedLogs.map((log, index) => `${log.timestampMs}-${index}`)));
  };

  const handleCollapseAll = () => {
    setExpandedKeys(new Set());
  };

  const toggleRowExpanded = (rowKey: string) => {
    setExpandedRowKeys((prev) => {
      const next = new Set(prev);
      if (next.has(rowKey)) next.delete(rowKey);
      else next.add(rowKey);
      return next;
    });
  };

  const renderPagination = () => {
    if (estimatedTotalPages <= 1 && !hasMore) return null;

    const pageButtons: (number | "ellipsis")[] = [];
    if (estimatedTotalPages <= 7) {
      for (let i = 1; i <= estimatedTotalPages; i++) pageButtons.push(i);
    } else {
      pageButtons.push(1);
      if (safePage > 3) pageButtons.push("ellipsis");
      const start = Math.max(2, safePage - 1);
      const end = Math.min(estimatedTotalPages - 1, safePage + 1);
      for (let i = start; i <= end; i++) pageButtons.push(i);
      if (safePage < estimatedTotalPages - 2) pageButtons.push("ellipsis");
      pageButtons.push(estimatedTotalPages);
    }

    return (
      <div className="flex flex-col sm:flex-row items-center justify-between gap-3 pt-3 border-t border-default-200">
        <div className="flex items-center gap-2 text-xs text-muted">
          <span>
            显示第 {startRecordIndex}–{endRecordIndex} 条（当前已载入 {logs.length} 条
            {totalInFile > logs.length ? `，总共 ${totalInFile} 条` : ""}）
          </span>
          {loadingChunk && (
            <span className="inline-flex items-center gap-1 text-[11px] text-accent font-medium ml-2">
              <Spinner size="sm" color="current" />
              正在分片加载下一批...
            </span>
          )}
        </div>

        <Pagination size="sm" className="w-auto">
          <Pagination.Content>
            <Pagination.Item>
              <Pagination.Previous
                isDisabled={safePage === 1}
                onPress={() => void handlePageChange(Math.max(1, safePage - 1))}
              >
                <Pagination.PreviousIcon />
                <span>上一页</span>
              </Pagination.Previous>
            </Pagination.Item>

            {pageButtons.map((p, idx) =>
              p === "ellipsis" ? (
                <Pagination.Item key={`ellipsis-${idx}`}>
                  <Pagination.Ellipsis />
                </Pagination.Item>
              ) : (
                <Pagination.Item key={p}>
                  <Pagination.Link
                    isActive={p === safePage}
                    onPress={() => void handlePageChange(p)}
                  >
                    {p}
                  </Pagination.Link>
                </Pagination.Item>
              ),
            )}

            <Pagination.Item>
              <Pagination.Next
                isDisabled={
                  (safePage === estimatedTotalPages && !hasMore) ||
                  (loadingChunk && paginatedLogs.length === 0)
                }
                onPress={() => void handlePageChange(safePage + 1)}
              >
                <span>下一页</span>
                <Pagination.NextIcon />
              </Pagination.Next>
            </Pagination.Item>
          </Pagination.Content>
        </Pagination>
      </div>
    );
  };

  if (!mode.enabled && !busy) {
    return (
      <PageShell centered>
        <Card title="调试模式未启用" description="请返回首页，连续点击左上角应用图标 5 次启用">
          <Button data-webview-navigation onPress={() => void navigation.back()} className="mt-2">
            返回工作台
          </Button>
        </Card>
      </PageShell>
    );
  }

  return (
    <PageShell maxWidth="xl">
      <div className="grid gap-6">
        <ModuleHeader
          title="诊断日志"
          eyebrow="DEVELOPER DIAGNOSTICS"
          description="仅记录脱敏的 Android 网络与门户探测信息"
        />

        <Card
          title="调试模式已启用"
          description={`当前已载入 ${logs.length} 条日志${totalInFile > logs.length ? `（文件总计 ${totalInFile} 条）` : ""} · 手动刷新拉取最新状态`}
        >
          <div className="flex flex-wrap gap-2.5">
            <Button variant="outline" onPress={() => void load()} isDisabled={busy}>
              {busy ? <Spinner size="sm" color="current" /> : <LuRefreshCw aria-hidden="true" />}
              手动刷新
            </Button>
            {hasMore && (
              <Button
                variant="outline"
                onPress={() => void handleLoadMoreChunkManual()}
                isDisabled={loadingChunk}
              >
                {loadingChunk && <Spinner size="sm" color="current" />}
                分片加载更多 (+100条)
              </Button>
            )}
            <Button variant="outline" onPress={() => void handleCopy(logs)}>
              <LuCopy aria-hidden="true" />
              复制全部 ({logs.length})
            </Button>
            {(search || selectedLevel !== "ALL") && (
              <Button variant="outline" onPress={() => void handleCopy(filteredLogs)}>
                <LuCopy aria-hidden="true" />
                复制筛选结果 ({filteredLogs.length})
              </Button>
            )}
            <Button variant="outline" onPress={() => void handleClear()}>
              <LuEraser aria-hidden="true" />
              清空日志
            </Button>
            <Button variant="danger" onPress={() => void handleDisable()}>
              <LuPower aria-hidden="true" />
              关闭调试模式
            </Button>
          </div>
        </Card>

        {message && (
          <Alert role="status" status="default">
            <Alert.Content>{message}</Alert.Content>
          </Alert>
        )}

        <Card title="最近记录">
          {busy && logs.length === 0 ? (
            <div className="flex items-center justify-center py-8">
              <Spinner />
            </div>
          ) : logs.length === 0 ? (
            <p className="text-[var(--szu-muted)] text-center py-8">
              暂无诊断日志。请返回校园网页面等待下一次网络探测。
            </p>
          ) : (
            <div className="space-y-4">
              <div className="flex flex-col gap-3">
                <div className="flex flex-col sm:flex-row gap-3 items-stretch sm:items-center justify-between">
                  <SearchField
                    aria-label="搜索诊断日志"
                    value={search}
                    onChange={(val) => {
                      setSearch(val);
                      setCurrentPage(1);
                    }}
                    className="w-full sm:max-w-md"
                  >
                    <SearchField.Group>
                      <SearchField.SearchIcon />
                      <SearchField.Input placeholder="搜索日志内容、SSID、IP、阶段等..." />
                      <SearchField.ClearButton />
                    </SearchField.Group>
                  </SearchField>

                  <div className="flex items-center gap-1.5 overflow-x-auto pb-1 sm:pb-0">
                    {(["ALL", "INFO", "WARN", "ERROR"] as const).map((level) => {
                      const count = levelCounts[level] ?? 0;
                      const isSelected = selectedLevel === level;
                      return (
                        <Button
                          key={level}
                          size="sm"
                          variant={isSelected ? "primary" : "outline"}
                          onPress={() => {
                            setSelectedLevel(level);
                            setCurrentPage(1);
                          }}
                          className="text-xs shrink-0"
                          aria-label={`级别筛选 ${level === "ALL" ? "全部" : level}`}
                        >
                          {level === "ALL" ? "全部" : level} ({count})
                        </Button>
                      );
                    })}
                  </div>
                </div>

                <div className="flex items-center justify-between flex-wrap gap-2 text-xs text-muted pt-1">
                  <div className="flex items-center gap-2">
                    <span>
                      {search || selectedLevel !== "ALL"
                        ? `找到 ${totalFiltered} 条匹配日志（已加载 ${logs.length} 条）`
                        : `共载入 ${logs.length} 条日志`}
                    </span>
                    <div className="flex items-center gap-1 ml-2">
                      <span>每页:</span>
                      {PAGE_SIZE_OPTIONS.map((size) => (
                        <button
                          key={size}
                          type="button"
                          onClick={() => {
                            setPageSize(size);
                            setCurrentPage(1);
                            if (logsLengthRef.current < size && hasMoreRef.current) {
                              void fetchNextChunk(size);
                            }
                          }}
                          className={`px-1.5 py-0.5 rounded text-xs transition-colors ${
                            pageSize === size
                              ? "bg-primary text-primary-foreground font-semibold"
                              : "hover:bg-default-200 text-muted"
                          }`}
                        >
                          {size}
                        </button>
                      ))}
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    <div className="flex items-center gap-1 bg-default-100 rounded-lg p-0.5">
                      <Button
                        size="sm"
                        variant={viewMode === "table" ? "primary" : "ghost"}
                        onPress={() => setViewMode("table")}
                        className="text-xs h-7 px-2.5"
                      >
                        <LuTable aria-hidden="true" />
                        表格视图
                      </Button>
                      <Button
                        size="sm"
                        variant={viewMode === "accordion" ? "primary" : "ghost"}
                        onPress={() => setViewMode("accordion")}
                        className="text-xs h-7 px-2.5"
                      >
                        <LuListCollapse aria-hidden="true" />
                        折叠面板
                      </Button>
                    </div>

                    {viewMode === "accordion" && paginatedLogs.length > 0 && (
                      <div className="flex items-center gap-1">
                        <Button
                          size="sm"
                          variant="outline"
                          onPress={handleExpandAll}
                          className="text-xs h-7 px-2"
                        >
                          全部展开
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onPress={handleCollapseAll}
                          className="text-xs h-7 px-2"
                        >
                          全部折叠
                        </Button>
                      </div>
                    )}
                  </div>
                </div>
              </div>

              {filteredLogs.length === 0 ? (
                loadingChunk ? (
                  <div className="flex flex-col items-center justify-center py-8 gap-3 text-center">
                    <Spinner size="md" />
                    <p className="text-muted text-sm">正在加载诊断日志分片...</p>
                  </div>
                ) : (
                  <div className="flex flex-col items-center justify-center py-8 gap-3 text-center">
                    <p className="text-muted text-sm">未找到匹配筛选条件的诊断日志</p>
                    <Button
                      size="sm"
                      variant="outline"
                      onPress={() => {
                        setSearch("");
                        setSelectedLevel("ALL");
                        setCurrentPage(1);
                      }}
                    >
                      清除筛选
                    </Button>
                  </div>
                )
              ) : paginatedLogs.length === 0 && loadingChunk ? (
                <div className="flex flex-col items-center justify-center py-8 gap-3 text-center">
                  <Spinner size="md" />
                  <p className="text-muted text-sm">正在加载第 {safePage} 页日志分片...</p>
                </div>
              ) : viewMode === "table" ? (
                /* Paginated Table View */
                <Table aria-label="诊断日志数据表格">
                  <Table.ScrollContainer className="overflow-auto">
                    <Table.Content aria-label="诊断日志数据表格" className="min-w-[760px]">
                      <Table.Header>
                        <Table.Column isRowHeader className="w-24">
                          时间
                        </Table.Column>
                        <Table.Column className="w-20">级别</Table.Column>
                        <Table.Column className="w-36">来源 / 阶段</Table.Column>
                        <Table.Column className="w-48">网络与环境</Table.Column>
                        <Table.Column>详细信息与消息</Table.Column>
                        <Table.Column className="w-24 text-right">操作</Table.Column>
                      </Table.Header>
                      <Table.Body>
                        {paginatedLogs.map((log, index) => {
                          const rowKey = `table-${log.timestampMs}-${index}`;
                          const isExpanded = expandedRowKeys.has(rowKey);
                          const net = [
                            log.connected == null ? null : log.connected ? "已连接" : "未连接",
                            log.transport,
                            log.ssid ? `SSID=${log.ssid}` : null,
                            log.ip ? `IP=${log.ip}` : null,
                            log.capabilitiesAvailable == null
                              ? null
                              : `能力=${log.capabilitiesAvailable ? "可用" : "不可用"}`,
                            log.wifiPermission ? `权限=${log.wifiPermission}` : null,
                            log.locationEnabled == null
                              ? null
                              : `位置=${log.locationEnabled ? "开" : "关"}`,
                          ]
                            .filter(Boolean)
                            .join(" ");

                          const details = [
                            log.portal ? `门户=${log.portal}` : null,
                            log.httpStatus != null ? `HTTP=${log.httpStatus}` : null,
                            log.elapsedMs != null ? `${log.elapsedMs}ms` : null,
                            log.errorKind ? `错误=${log.errorKind}` : null,
                            log.message,
                          ]
                            .filter(Boolean)
                            .join(" ");

                          return (
                            <Table.Row key={rowKey} id={rowKey}>
                              <Table.Cell className="text-xs font-mono text-muted whitespace-nowrap">
                                {new Date(log.timestampMs).toLocaleTimeString()}
                              </Table.Cell>
                              <Table.Cell>
                                <Chip size="sm" color={getLevelColor(log.level)} variant="primary">
                                  {log.level.toUpperCase()}
                                </Chip>
                              </Table.Cell>
                              <Table.Cell className="text-xs font-mono font-medium whitespace-nowrap">
                                <span className="font-mono">
                                  {log.source}/{log.stage}
                                  {net ? ` · ${net}` : ""}
                                </span>
                              </Table.Cell>
                              <Table.Cell className="text-xs font-mono break-all whitespace-normal">
                                {net || "—"}
                              </Table.Cell>
                              <Table.Cell className="text-xs font-mono break-all whitespace-pre-wrap select-text">
                                <div>{details || "—"}</div>
                                {isExpanded && (
                                  <div className="mt-2 pt-2 border-t border-default-200 space-y-2">
                                    {log.message && (
                                      <div className="rounded bg-default-100 p-2 font-mono text-xs text-foreground">
                                        <span className="font-bold text-muted mr-1">详细消息:</span>
                                        {log.message}
                                      </div>
                                    )}
                                    <div className="grid grid-cols-2 gap-1 text-[11px] text-muted font-mono">
                                      {getLogDetails(log).map((d) => (
                                        <div key={d.label}>
                                          <span className="font-semibold">{d.label}:</span>{" "}
                                          <span>{String(d.value)}</span>
                                        </div>
                                      ))}
                                    </div>
                                  </div>
                                )}
                              </Table.Cell>
                              <Table.Cell className="text-right">
                                <div className="flex items-center justify-end gap-1">
                                  <Button
                                    size="sm"
                                    variant="ghost"
                                    isIconOnly
                                    aria-label={isExpanded ? "收起详情" : "展开详情"}
                                    onPress={() => toggleRowExpanded(rowKey)}
                                  >
                                    {isExpanded ? (
                                      <LuChevronUp aria-hidden="true" />
                                    ) : (
                                      <LuChevronDown aria-hidden="true" />
                                    )}
                                  </Button>
                                  <Button
                                    size="sm"
                                    variant="ghost"
                                    isIconOnly
                                    aria-label="复制此条日志"
                                    onPress={() => void handleCopySingle(log)}
                                  >
                                    <LuCopy aria-hidden="true" />
                                  </Button>
                                </div>
                              </Table.Cell>
                            </Table.Row>
                          );
                        })}
                      </Table.Body>
                    </Table.Content>
                  </Table.ScrollContainer>
                </Table>
              ) : (
                /* Paginated Accordion View */
                <Accordion
                  allowsMultipleExpanded
                  expandedKeys={expandedKeys}
                  onExpandedChange={(keys) => {
                    setExpandedKeys(new Set(Array.from(keys, String)));
                  }}
                  className="w-full"
                >
                  {paginatedLogs.map((log, index) => {
                    const itemKey = `${log.timestampMs}-${index}`;
                    const summary = getLogSummary(log);
                    return (
                      <Accordion.Item key={itemKey} id={itemKey}>
                        <Accordion.Heading>
                          <Accordion.Trigger className="w-full text-left py-3 px-3 hover:bg-default-100/60 rounded-lg transition-colors">
                            <div className="flex flex-col gap-1.5 w-full min-w-0 pr-2">
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-xs text-muted font-mono shrink-0">
                                  {new Date(log.timestampMs).toLocaleTimeString()}
                                </span>
                                <Chip size="sm" color={getLevelColor(log.level)} variant="primary">
                                  {log.level.toUpperCase()}
                                </Chip>
                                {log.portal && (
                                  <Chip size="sm" variant="soft" className="text-[11px]">
                                    {log.portal}
                                  </Chip>
                                )}
                                {log.httpStatus != null && (
                                  <Chip
                                    size="sm"
                                    color={log.httpStatus >= 400 ? "danger" : "success"}
                                    variant="soft"
                                    className="text-[11px]"
                                  >
                                    HTTP {log.httpStatus}
                                  </Chip>
                                )}
                                {log.elapsedMs != null && (
                                  <span className="text-[11px] text-muted font-mono">
                                    {log.elapsedMs}ms
                                  </span>
                                )}
                              </div>
                              <div className="text-xs font-mono text-foreground break-all whitespace-normal">
                                {log.source}/{log.stage}
                                {summary ? ` - ${summary}` : ""}
                              </div>
                            </div>
                            <Accordion.Indicator />
                          </Accordion.Trigger>
                        </Accordion.Heading>
                        <Accordion.Panel>
                          <Accordion.Body className="space-y-3 pt-2 pb-4">
                            {log.message && (
                              <div className="rounded-lg bg-default-100 p-3 text-xs font-mono break-all whitespace-pre-wrap select-text border border-default-200">
                                <div className="font-semibold text-muted mb-1 text-[11px]">
                                  消息详情
                                </div>
                                <div className="text-foreground leading-relaxed">{log.message}</div>
                              </div>
                            )}

                            <Table aria-label={`${log.source}/${log.stage} 字段详情`}>
                              <Table.ScrollContainer>
                                <Table.Content aria-label={`${log.source}/${log.stage} 字段详情`}>
                                  <Table.Header>
                                    <Table.Column isRowHeader className="w-28 text-xs">
                                      字段
                                    </Table.Column>
                                    <Table.Column className="text-xs">值</Table.Column>
                                  </Table.Header>
                                  <Table.Body>
                                    {getLogDetails(log).map((field) => (
                                      <Table.Row key={field.label} id={field.label}>
                                        <Table.Cell className="font-semibold text-muted text-xs whitespace-nowrap">
                                          {field.label}
                                        </Table.Cell>
                                        <Table.Cell className="text-xs font-mono break-all whitespace-pre-wrap select-text">
                                          {String(field.value)}
                                        </Table.Cell>
                                      </Table.Row>
                                    ))}
                                  </Table.Body>
                                </Table.Content>
                              </Table.ScrollContainer>
                            </Table>

                            <div className="flex items-center justify-between pt-1">
                              <span className="text-[11px] text-muted font-mono">
                                时间戳: {log.timestampMs}
                              </span>
                              <Button
                                size="sm"
                                variant="outline"
                                onPress={() => void handleCopySingle(log)}
                              >
                                <LuCopy aria-hidden="true" />
                                复制单条
                              </Button>
                            </div>
                          </Accordion.Body>
                        </Accordion.Panel>
                      </Accordion.Item>
                    );
                  })}
                </Accordion>
              )}

              {/* Pagination Controls */}
              {renderPagination()}
            </div>
          )}
        </Card>
      </div>
    </PageShell>
  );
}

export { formatLog, formatLogs, copyText };
