import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Alert, Button, Card, CloseButton, Spinner } from "@heroui/react";
import { LuArrowLeft, LuCopy, LuEraser, LuPower } from "react-icons/lu";
import { PageShell } from "./components";
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

const initialMode: DebugMode = { enabled: false, logCount: 0 };

function formatLog(log: DebugLog) {
  const time = new Date(log.timestampMs).toLocaleString();
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
  const detail = [network, portal, http, elapsed, error, log.message].filter(Boolean).join(" ");
  return `[${time}] ${log.level.toUpperCase()} ${log.source}/${log.stage}${detail ? ` - ${detail}` : ""}`;
}

function formatLogs(logs: DebugLog[]) {
  return logs.length ? logs.map(formatLog).join("\n") : "暂无诊断日志";
}

export default function DebugLogPage() {
  const navigation = useLogicalNavigation();
  const [mode, setMode] = useState(initialMode);
  const [logs, setLogs] = useState<DebugLog[]>([]);
  const [busy, setBusy] = useState(true);
  const [message, setMessage] = useState<string>();

  const load = async () => {
    setBusy(true);
    try {
      const nextMode = (await invoke("get_debug_mode")) as DebugMode;
      setMode(nextMode);
      setLogs(nextMode.enabled ? ((await invoke("get_debug_logs")) as DebugLog[]) : []);
    } catch (cause) {
      setMessage(`无法读取诊断日志：${String(cause)}`);
    } finally {
      setBusy(false);
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
      if (!event.payload.enabled) setLogs([]);
    }).then((stop) => {
      if (active) unlistenMode = stop;
      else stop();
    });
    void listen<DebugLog>("debug-log-added", (event) => {
      if (!active) return;
      setLogs((current) => [...current, event.payload].slice(-1000));
      setMode((current) => ({ ...current, logCount: current.logCount + 1 }));
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

  const renderedLogs = useMemo(() => [...logs].reverse(), [logs]);

  const handleCopy = async () => {
    setMessage(
      (await copyText(formatLogs(logs))) ? "日志已复制" : "无法自动复制，请长按日志文本手动复制",
    );
  };

  const handleClear = async () => {
    try {
      await invoke("clear_debug_logs");
      setLogs([]);
      setMode((current) => ({ ...current, logCount: 0 }));
      setMessage("日志已清空");
    } catch (cause) {
      setMessage(`清空失败：${String(cause)}`);
    }
  };

  const handleDisable = async () => {
    try {
      await invoke("set_debug_mode", { enabled: false });
      await navigation.back();
    } catch (cause) {
      setMessage(`关闭调试模式失败：${String(cause)}`);
    }
  };

  if (!mode.enabled && !busy) {
    return (
      <PageShell centered>
        <Card className="debug-card" variant="default">
          <Card.Content>
            <Card.Title>调试模式未启用</Card.Title>
            <Card.Description>请返回首页，连续点击左上角应用图标 5 次启用。</Card.Description>
            <Button data-webview-navigation onPress={() => void navigation.back()}>
              返回工作台
            </Button>
          </Card.Content>
        </Card>
      </PageShell>
    );
  }

  return (
    <PageShell>
      <div className="debug-layout">
        <CloseButton
          data-webview-navigation
          className="debug-back"
          aria-label="返回工作台"
          onPress={() => void navigation.back()}
        >
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <header className="debug-header">
          <span data-webview-navigation className="eyebrow">
            DEVELOPER DIAGNOSTICS
          </span>
          <h1 data-webview-navigation>诊断日志</h1>
          <p className="page-description">仅记录脱敏的 Android 网络与门户探测信息。</p>
        </header>
        <Card className="debug-card" variant="default">
          <Card.Header>
            <Card.Title>调试模式已启用</Card.Title>
            <Card.Description>当前保留 {mode.logCount} 条日志，最多保存 1000 条。</Card.Description>
          </Card.Header>
          <Card.Content className="debug-actions">
            <Button variant="outline" onPress={() => void load()} isDisabled={busy}>
              {busy && <Spinner size="sm" color="current" />}
              刷新
            </Button>
            <Button variant="outline" onPress={() => void handleCopy()}>
              <LuCopy aria-hidden="true" />
              复制全部
            </Button>
            <Button variant="outline" onPress={() => void handleClear()}>
              <LuEraser aria-hidden="true" />
              清空日志
            </Button>
            <Button variant="danger" onPress={() => void handleDisable()}>
              <LuPower aria-hidden="true" />
              关闭调试模式
            </Button>
          </Card.Content>
        </Card>
        {message && (
          <Alert role="status" status="default">
            <Alert.Content>{message}</Alert.Content>
          </Alert>
        )}
        <Card className="debug-card" variant="default">
          <Card.Header>
            <Card.Title>最近记录</Card.Title>
          </Card.Header>
          <Card.Content>
            {busy ? (
              <div className="debug-empty">
                <Spinner />
              </div>
            ) : renderedLogs.length ? (
              <ol className="debug-log-list">
                {renderedLogs.map((log, index) => (
                  <li key={`${log.timestampMs}-${index}`}>{formatLog(log)}</li>
                ))}
              </ol>
            ) : (
              <p className="debug-empty">暂无诊断日志。请返回校园网页面等待下一次网络探测。</p>
            )}
          </Card.Content>
        </Card>
      </div>
    </PageShell>
  );
}

export { formatLog, formatLogs, copyText };
