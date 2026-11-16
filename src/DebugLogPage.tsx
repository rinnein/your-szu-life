import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Card, LoadingIndicator, Button, Radios, Radio, Switch } from "material/react";
import { Banner, Chip, SettingList, SettingRow, toast } from "./ui";
import { setAnalyticsEnabled as setAnalyticsTracking } from "./analytics";
import { LuFileText, LuFolderOpen, LuPower, LuRefreshCw, LuTrash2 } from "react-icons/lu";
import { isAndroidPlatform } from "./os-info";
import { errorMessage, useAsyncLoad } from "./async-state";
import { usePending } from "./use-pending";

type DebugMode = { enabled: boolean; demoMode?: boolean; logCount: number };

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

type AnalyticsSettings = {
  enabled: boolean;
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export default function DebugLogPage() {
  const [mode, setMode] = useState<DebugMode | null>(null);
  const [isAndroid, setIsAndroid] = useState(false);
  const [analyticsEnabled, setAnalyticsEnabledState] = useState(true);
  const logs = useAsyncLoad(() => invoke<LogInfo>("get_log_info"));
  const pending = usePending();

  useEffect(() => {
    let active = true;
    void isAndroidPlatform().then((value) => {
      if (active) setIsAndroid(value);
    });

    void invoke<DebugMode>("get_debug_mode")
      .then((value) => {
        if (active) setMode(value);
      })
      .catch((cause) => {
        if (!active) return;
        // 读取失败时仍按未启用处理，避免把整个调试面板暴露出来。
        setMode({ enabled: false, logCount: 0 });
        toast.danger("无法读取调试模式状态", { description: errorMessage(cause) });
      });

    void invoke<AnalyticsSettings>("get_analytics_settings")
      .then((settings) => {
        if (!active) return;
        const enabled = settings?.enabled !== false;
        setAnalyticsEnabledState(enabled);
        setAnalyticsTracking(enabled);
      })
      .catch(() => {
        // 读取失败时保持默认开启，实际发送仍由后端偏好决定。
      });

    const unlistenPromise = listen<DebugMode>("debug-mode-changed", (event) => {
      if (!active) return;
      setMode(event.payload);
      if (event.payload.enabled) void logs.reload();
    });

    const unlistenAnalytics = listen<AnalyticsSettings>("analytics-settings-changed", (event) => {
      if (!active) return;
      const enabled = event.payload?.enabled !== false;
      setAnalyticsEnabledState(enabled);
      setAnalyticsTracking(enabled);
    });

    return () => {
      active = false;
      void unlistenPromise.then((fn) => fn());
      void Promise.resolve(unlistenAnalytics)
        .then((fn) => fn?.())
        .catch(() => undefined);
    };
  }, [logs.reload]);

  const logInfo =
    logs.state.status === "ready"
      ? logs.state.data
      : logs.state.status === "loading"
        ? logs.state.previous
        : logs.state.status === "error"
          ? logs.state.previous
          : undefined;

  const handleOpenTodayLog = async () => {
    const path = logInfo?.todayLogPath;
    if (!path) {
      toast.warning("今日日志文件路径不可用");
      return;
    }
    const result = await pending.run("open", () => invoke("open_log_file", { path }));
    if (!result.ok && !result.skipped) {
      toast.danger("打开日志文件失败", { description: errorMessage(result.error) });
    }
  };

  const handleRevealLogDir = async () => {
    const path = logInfo?.todayLogPath;
    if (!path) return;
    const result = await pending.run("open", () => revealItemInDir(path));
    if (!result.ok && !result.skipped) {
      toast.danger("定位日志文件失败", { description: errorMessage(result.error) });
    }
  };

  const handleClearLogs = async () => {
    const result = await pending.run("clear", async () => {
      await invoke("clear_debug_logs");
      await logs.reload();
    });
    if (result.ok) {
      toast.success("日志已清理");
    } else if (!result.skipped) {
      toast.danger("清空日志失败", { description: errorMessage(result.error) });
    }
  };

  const handleSetLogLevel = async (level: string) => {
    const result = await pending.run("level", () => invoke<string>("set_log_level", { level }));
    if (result.ok) {
      await logs.reload();
      toast.success(`日志保留级别已更新为 ${result.value}`);
    } else if (!result.skipped) {
      toast.danger("设置日志级别失败", { description: errorMessage(result.error) });
    }
  };

  const handleAnalyticsToggle = async (enabled: boolean) => {
    if (pending.isPending("analytics")) return;
    const previous = analyticsEnabled;
    setAnalyticsEnabledState(enabled);
    setAnalyticsTracking(enabled);
    const result = await pending.run("analytics", () =>
      invoke<AnalyticsSettings>("save_analytics_settings", { settings: { enabled } }),
    );
    if (result.ok) {
      const next = result.value?.enabled !== false;
      setAnalyticsEnabledState(next);
      setAnalyticsTracking(next);
      toast.success(next ? "匿名统计已开启" : "匿名统计已关闭");
    } else if (!result.skipped) {
      setAnalyticsEnabledState(previous);
      setAnalyticsTracking(previous);
      toast.danger("保存统计设置失败", { description: errorMessage(result.error) });
    }
  };

  const handleDisable = async () => {
    const exitingDemo = mode?.demoMode === true;
    const result = await pending.run("disable", () => exitDebugMode(exitingDemo));
    if (result.ok) {
      setMode((current) => (current ? { ...current, enabled: false, demoMode: false } : current));
      toast.info(exitingDemo ? "已退出演示模式" : "调试模式已关闭");
    } else if (!result.skipped) {
      toast.danger("关闭调试模式失败", { description: errorMessage(result.error) });
    }
  };

  const handleRestartOnboarding = async () => {
    const result = await pending.run("restart", () => invoke("restart_onboarding"));
    if (result.ok) {
      toast.success("已重新开始初始化设置", {
        description: "已保存的账户、外观、通知、模型等设置不会被重置。",
      });
    } else if (!result.skipped) {
      toast.danger("重新开始初始化设置失败", { description: errorMessage(result.error) });
    }
  };

  if (mode === null) {
    return (
      <Card variant="outlined">
        <div className="status-line" role="status">
          <LoadingIndicator size={20} />
          <span>正在读取调试模式…</span>
        </div>
      </Card>
    );
  }

  if (!mode.enabled) {
    // 正常路径下这个 tab 只在调试模式启用后出现；这里只兜底刚关闭调试模式的那一帧。
    return (
      <Card variant="outlined">
        <div slot="headline">调试模式未启用</div>
        <div slot="subhead">在首页连续点击左上角应用图标 5 次即可启用</div>
      </Card>
    );
  }

  if (!logInfo) {
    return (
      <Banner status="danger">
        <Banner.Title>无法读取日志信息</Banner.Title>
        <Banner.Description>
          {logs.state.status === "error" ? logs.state.message : "日志信息尚未就绪。"}
        </Banner.Description>
        <div className="mt-3">
          <Button
            variant="text"
            onClick={() => void logs.reload()}
            disabled={logs.state.status === "loading"}
          >
            重试
          </Button>
        </div>
      </Banner>
    );
  }

  const levels = logInfo.availableLevels.length
    ? logInfo.availableLevels
    : ["DEBUG", "INFO", "WARN", "ERROR"];
  const currentLevel = logInfo.currentLevel.toUpperCase();
  const loading = logs.state.status === "loading";

  return (
    <>
      {mode.demoMode && (
        <Banner status="warning" className="w-full">
          <Banner.Title>演示模式进行中</Banner.Title>
          <Banner.Description>
            界面上的登录数据均为模拟数据，仅用于检查样式；退出后会回到初始化向导。
          </Banner.Description>
        </Banner>
      )}

      {logs.state.status === "error" && (
        <Banner status="danger">
          <Banner.Title>刷新日志信息失败</Banner.Title>
          <Banner.Description>{logs.state.message}</Banner.Description>
        </Banner>
      )}

      <Card variant="outlined">
        <div slot="headline">日志文件概览</div>
        <div slot="subhead">
          日志按天独立存储；Android 保存在应用专属外部目录，卸载时由系统清理。当本地日志总大小达到
          10MB 或单个日志文件保存超过 7 天时，系统会自动删除最旧的日志，保障磁盘空间不受占用。
        </div>
        <div className="grid gap-3">
          <div className="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
            <div className="flex flex-col gap-1 rounded-inner bg-surface-secondary p-3">
              <span className="text-xs font-medium text-muted">今日日志文件</span>
              <span className="select-all break-all font-mono text-xs text-foreground">
                {logInfo.todayLogPath || "未读取"}
              </span>
            </div>
            <div className="flex flex-col gap-1 rounded-inner bg-surface-secondary p-3">
              <span className="text-xs font-medium text-muted">日志存储目录</span>
              <span className="select-all break-all font-mono text-xs text-foreground">
                {logInfo.logDir || "未读取"}
              </span>
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Chip size="sm" variant="soft" color="accent">
                {logInfo.fileCount} 个历史文件
              </Chip>
              <Chip size="sm" variant="soft" color="default">
                总大小 {formatBytes(logInfo.totalSizeBytes)}
              </Chip>
              {loading && (
                <span className="status-line">
                  <LoadingIndicator size={14} />
                  <span>正在刷新</span>
                </span>
              )}
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                variant="filled"
                size="xs"
                className="flex items-center gap-1.5"
                onClick={() => void handleOpenTodayLog()}
                disabled={pending.isPending("open")}
              >
                <LuFileText className="size-4" />
                <span>打开今日日志</span>
              </Button>
              {!isAndroid && (
                <Button
                  variant="text"
                  size="xs"
                  className="flex items-center gap-1.5"
                  onClick={() => void handleRevealLogDir()}
                  disabled={pending.isPending("open")}
                >
                  <LuFolderOpen className="size-4" />
                  <span>在文件管理器中显示</span>
                </Button>
              )}
              <Button
                variant="tonal"
                size="xs"
                className="button-danger-soft flex items-center gap-1.5"
                onClick={() => void handleClearLogs()}
                disabled={pending.isPending("clear")}
              >
                {pending.isPending("clear") ? (
                  <LoadingIndicator size={16} />
                ) : (
                  <LuTrash2 className="size-4" />
                )}
                <span>清理日志</span>
              </Button>
              <Button
                variant="text"
                size="xs"
                aria-label="刷新日志信息"
                onClick={() => void logs.reload()}
                disabled={loading}
              >
                {loading ? <LoadingIndicator size={16} /> : <LuRefreshCw className="size-4" />}
              </Button>
            </div>
          </div>
        </div>
      </Card>

      <Card variant="outlined">
        <div slot="headline">日志文件保留级别</div>
        <div slot="subhead">
          控制写入日志文件的最低级别，默认为 WARN。选择 DEBUG
          会记录详细的探测与网络请求信息，因此日志体积也会明显增大。
        </div>
        <Radios
          aria-label="日志文件保留级别"
          defaultValue={currentLevel}
          disabled={pending.isPending("level")}
          onChange={(event) => {
            const value = event.detail?.value;
            if (value) void handleSetLogLevel(value);
          }}
        >
          {levels.map((level) => (
            <Radio key={level} value={level}>
              {level}
            </Radio>
          ))}
        </Radios>
        {pending.isPending("level") && (
          <p className="status-line">
            <LoadingIndicator size={16} />
            <span>正在保存日志保留级别…</span>
          </p>
        )}
      </Card>

      <Card variant="outlined">
        <div slot="headline">匿名使用统计</div>
        <div slot="subhead">
          仅发送应用启动/退出、页面路径、应用版本与系统类型等匿名信息，不包含账号、成绩或课程内容。
        </div>
        <SettingList>
          <SettingRow
            title="发送匿名统计"
            description="关闭后不再产生新的统计事件；已排队的事件可能在下一次发送前完成上报。"
          >
            <Switch
              checked={analyticsEnabled}
              disabled={pending.isPending("analytics")}
              onChange={(event: any) => {
                const checked = Boolean(event.detail?.checked ?? event.target?.checked);
                void handleAnalyticsToggle(checked);
              }}
              aria-label="匿名使用统计"
            />
          </SettingRow>
        </SettingList>
      </Card>

      <Card variant="outlined">
        <div slot="headline">初始化设置</div>
        <div slot="subhead">
          重新走一遍初始化向导。只重置步骤进度，不会清除账户、外观、通知、本地模型等已保存的设置。
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="outlined"
            disabled={pending.isPending("restart")}
            onClick={() => void handleRestartOnboarding()}
          >
            {pending.isPending("restart") ? (
              <LoadingIndicator size={16} />
            ) : (
              <LuRefreshCw className="size-4" />
            )}
            重新开始初始化向导
          </Button>
        </div>
      </Card>

      <Card variant="outlined">
        <div slot="headline">{mode.demoMode ? "退出演示模式" : "关闭调试模式"}</div>
        <div slot="subhead">
          {mode.demoMode
            ? "退出后会回到初始化向导，模拟数据不会保留。"
            : "关闭后调试相关入口会隐藏；在首页连续点击左上角应用图标 5 次可重新启用。"}
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="tonal"
            className="button-danger-soft flex items-center gap-1.5"
            disabled={pending.isPending("disable")}
            onClick={() => void handleDisable()}
          >
            {pending.isPending("disable") ? (
              <LoadingIndicator size={16} />
            ) : (
              <LuPower className="size-4" />
            )}
            <span>{mode.demoMode ? "退出演示模式" : "关闭调试模式"}</span>
          </Button>
        </div>
      </Card>
    </>
  );
}

function exitDebugMode(demoMode: boolean): Promise<unknown> {
  return demoMode
    ? invoke("set_demo_mode", { enabled: false })
    : invoke("set_debug_mode", { enabled: false });
}
