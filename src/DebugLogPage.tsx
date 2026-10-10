import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Alert, Button, Spinner } from "@heroui/react";
import { LuCopy, LuEraser, LuPower } from "react-icons/lu";
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
          description={`当前保留 ${mode.logCount} 条日志，最多保存 1000 条`}
        >
          <div className="flex flex-wrap gap-2.5">
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
          </div>
        </Card>

        {message && (
          <Alert role="status" status="default">
            <Alert.Content>{message}</Alert.Content>
          </Alert>
        )}

        <Card title="最近记录">
          {busy ? (
            <div className="flex items-center justify-center py-8">
              <Spinner />
            </div>
          ) : renderedLogs.length ? (
            <ol className="debug-log-list">
              {renderedLogs.map((log, index) => (
                <li key={`${log.timestampMs}-${index}`}>{formatLog(log)}</li>
              ))}
            </ol>
          ) : (
            <p className="text-[var(--szu-muted)] text-center py-8">
              暂无诊断日志。请返回校园网页面等待下一次网络探测。
            </p>
          )}
        </Card>
      </div>
    </PageShell>
  );
}

export { formatLog, formatLogs, copyText };
