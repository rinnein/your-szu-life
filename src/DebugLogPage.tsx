import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Alert, Button, Card, Chip, Spinner, toast } from "@heroui/react";
import { LuFileText, LuFolderOpen, LuPower, LuRefreshCw, LuTrash2 } from "react-icons/lu";
import { ModuleHeader, PageShell } from "./components";
import { isAndroidPlatform } from "./os-info";

type DebugMode = { enabled: boolean; logCount: number };

type LogFileInfo = {
  name: string;
  sizeBytes: number;
  modifiedMs: number;
};

type LogInfo = {
  todayLogPath: string;
  logDir: string;
  totalSizeBytes: number;
  fileCount: number;
  currentLevel: string;
  availableLevels: string[];
  files: LogFileInfo[];
};

const initialMode: DebugMode = { enabled: false, logCount: 0 };

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export default function DebugLogPage() {
  const navigation = useLogicalNavigation();
  const [mode, setMode] = useState<DebugMode>(initialMode);
  const [logInfo, setLogInfo] = useState<LogInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [settingLevel, setSettingLevel] = useState(false);
  const [isAndroid, setIsAndroid] = useState(false);

  const loadLogInfo = async () => {
    try {
      const info = await invoke<LogInfo>("get_log_info");
      setLogInfo(info);
    } catch (e) {
      toast.danger(`获取日志信息失败: ${String(e)}`);
    }
  };

  useEffect(() => {
    let active = true;
    void isAndroidPlatform().then((val) => {
      if (active) setIsAndroid(val);
    });

    void invoke<DebugMode>("get_debug_mode")
      .then((m) => {
        if (!active) return;
        setMode(m);
        if (m.enabled) {
          setBusy(true);
          void loadLogInfo().finally(() => {
            if (active) setBusy(false);
          });
        }
      })
      .catch((err) => {
        toast.danger(`无法读取调试模式状态: ${String(err)}`);
      });

    const unlistenPromise = listen<DebugMode>("debug-mode-changed", (event) => {
      if (!active) return;
      setMode(event.payload);
      if (event.payload.enabled) {
        void loadLogInfo();
      }
    });

    return () => {
      active = false;
      void unlistenPromise.then((fn) => fn());
    };
  }, []);

  const handleOpenTodayLog = async () => {
    if (!logInfo?.todayLogPath) {
      toast.warning("今日日志文件路径不可用");
      return;
    }
    try {
      await openPath(logInfo.todayLogPath);
    } catch (e) {
      toast.danger(`打开日志文件失败: ${String(e)}`);
    }
  };

  const handleRevealLogDir = async () => {
    if (!logInfo) return;
    try {
      if (isAndroid) {
        await openPath(logInfo.logDir);
      } else {
        await revealItemInDir(logInfo.todayLogPath);
      }
    } catch (e) {
      toast.danger(`定位日志文件失败: ${String(e)}`);
    }
  };

  const handleClearLogs = async () => {
    setClearing(true);
    try {
      await invoke("clear_debug_logs");
      await loadLogInfo();
      toast.success("日志已清理");
    } catch (cause) {
      toast.danger(`清空日志失败: ${String(cause)}`);
    } finally {
      setClearing(false);
    }
  };

  const handleSetLogLevel = async (level: string) => {
    if (settingLevel || logInfo?.currentLevel === level) return;
    setSettingLevel(true);
    try {
      const updated = await invoke<string>("set_log_level", { level });
      setLogInfo((prev) => (prev ? { ...prev, currentLevel: updated } : prev));
      toast.success(`日志保留级别已更新为 ${updated}`);
    } catch (e) {
      toast.danger(`设置日志级别失败: ${String(e)}`);
    } finally {
      setSettingLevel(false);
    }
  };

  const handleDisable = async () => {
    try {
      await invoke("set_debug_mode", { enabled: false });
      toast.info("调试模式已关闭");
      await navigation.back();
    } catch (cause) {
      toast.danger(`关闭调试模式失败: ${String(cause)}`);
    }
  };

  if (!mode.enabled && !busy) {
    return (
      <PageShell centered>
        <Card variant="default">
          <Card.Header>
            <Card.Title>调试模式未启用</Card.Title>
            <Card.Description>请返回首页，连续点击左上角应用图标 5 次启用</Card.Description>
          </Card.Header>
          <Card.Content>
            <Button data-webview-navigation onPress={() => void navigation.back()} className="mt-2">
              返回工作台
            </Button>
          </Card.Content>
        </Card>
      </PageShell>
    );
  }

  const levels = logInfo?.availableLevels ?? ["DEBUG", "INFO", "WARN", "ERROR"];

  return (
    <PageShell maxWidth="xl">
      <div className="grid gap-6">
        <ModuleHeader
          title="诊断日志"
          eyebrow="DEVELOPER DIAGNOSTICS & LOGS"
          description="应用所有日志均由系统日志服务统一接管，按日期归档于本地"
        />

        {busy && !logInfo ? (
          <div className="flex items-center justify-center py-12">
            <Spinner size="lg" />
            <span className="ml-3 text-sm text-muted">正在载入日志状态…</span>
          </div>
        ) : (
          <>
            <Card variant="default">
              <Card.Header>
                <Card.Title>日志文件概览</Card.Title>
                <Card.Description>日志按天独立存储，可通过系统关联程序直接打开</Card.Description>
              </Card.Header>
              <Card.Content className="grid gap-4">
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
                  <div className="flex flex-col gap-1 p-3 rounded-lg bg-default-100/60 border border-default-200">
                    <span className="text-xs text-muted font-medium">今日日志文件</span>
                    <span className="font-mono text-xs break-all select-all text-foreground">
                      {logInfo?.todayLogPath || "未读取"}
                    </span>
                  </div>
                  <div className="flex flex-col gap-1 p-3 rounded-lg bg-default-100/60 border border-default-200">
                    <span className="text-xs text-muted font-medium">日志存储目录</span>
                    <span className="font-mono text-xs break-all select-all text-foreground">
                      {logInfo?.logDir || "未读取"}
                    </span>
                  </div>
                </div>

                <div className="flex flex-wrap items-center justify-between gap-3 pt-2">
                  <div className="flex items-center gap-2">
                    <Chip size="sm" variant="soft" color="accent">
                      {logInfo ? `${logInfo.fileCount} 个历史文件` : "读取中"}
                    </Chip>
                    <Chip size="sm" variant="soft" color="default">
                      {logInfo ? `总大小 ${formatBytes(logInfo.totalSizeBytes)}` : "读取中"}
                    </Chip>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      variant="primary"
                      size="sm"
                      className="flex items-center gap-1.5"
                      onPress={() => void handleOpenTodayLog()}
                      isDisabled={!logInfo}
                    >
                      <LuFileText className="size-4" />
                      <span>打开今日日志</span>
                    </Button>
                    {!isAndroid && (
                      <Button
                        variant="secondary"
                        size="sm"
                        className="flex items-center gap-1.5"
                        onPress={() => void handleRevealLogDir()}
                        isDisabled={!logInfo}
                      >
                        <LuFolderOpen className="size-4" />
                        <span>在文件管理器中显示</span>
                      </Button>
                    )}
                    <Button
                      variant="danger"
                      size="sm"
                      className="flex items-center gap-1.5"
                      onPress={() => void handleClearLogs()}
                      isDisabled={clearing || !logInfo}
                    >
                      {clearing ? (
                        <Spinner size="sm" color="current" />
                      ) : (
                        <LuTrash2 className="size-4" />
                      )}
                      <span>清理日志</span>
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      isIconOnly
                      aria-label="刷新日志信息"
                      onPress={() => void loadLogInfo()}
                      isDisabled={busy}
                    >
                      <LuRefreshCw className={`size-4 ${busy ? "animate-spin" : ""}`} />
                    </Button>
                  </div>
                </div>
              </Card.Content>
            </Card>

            <Card variant="default">
              <Card.Header>
                <Card.Title>日志文件保留级别</Card.Title>
                <Card.Description>
                  控制写入日志文件的最低级别，默认为 INFO。开启 DEBUG
                  将记录详细的探测与网络请求信息。
                </Card.Description>
              </Card.Header>
              <Card.Content className="grid gap-3">
                <div className="flex flex-wrap gap-2 items-center">
                  {levels.map((lvl) => {
                    const isActive = logInfo?.currentLevel.toUpperCase() === lvl;
                    return (
                      <Button
                        key={lvl}
                        size="sm"
                        variant={isActive ? "primary" : "outline"}
                        className={`min-w-20 font-mono text-xs ${
                          isActive ? "font-semibold shadow-xs" : ""
                        }`}
                        isDisabled={settingLevel}
                        onPress={() => void handleSetLogLevel(lvl)}
                      >
                        {lvl}
                        {lvl === "INFO" && !isActive && (
                          <span className="text-[10px] opacity-70 ml-1">(默认)</span>
                        )}
                        {isActive && <span className="text-[10px] ml-1">(当前)</span>}
                      </Button>
                    );
                  })}
                </div>
              </Card.Content>
            </Card>

            <Alert status="accent" className="w-full">
              <Alert.Indicator />
              <Alert.Content>
                <Alert.Title>自动归档与清理机制</Alert.Title>
                <Alert.Description>
                  应用内日志按天单独保存文件。当本地日志总大小达到 10MB 或单个日志文件保存超过 7
                  天时，系统会自动删除最旧的日志，保障磁盘空间不受占用。
                </Alert.Description>
              </Alert.Content>
            </Alert>

            <div className="flex justify-end pt-2">
              <Button
                variant="ghost"
                size="sm"
                className="text-danger flex items-center gap-1.5"
                onPress={() => void handleDisable()}
              >
                <LuPower className="size-4" />
                <span>关闭调试模式</span>
              </Button>
            </div>
          </>
        )}
      </div>
    </PageShell>
  );
}
