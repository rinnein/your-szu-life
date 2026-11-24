import { useCallback, useEffect, useMemo, type ReactNode } from "react";
import {
  LuAlarmClock,
  LuBatteryCharging,
  LuBellRing,
  LuCamera,
  LuGlobe,
  LuMapPin,
  LuPackagePlus,
  LuPower,
  LuRadio,
  LuRotateCcw,
  LuServer,
  LuWifi,
} from "react-icons/lu";
import { Button, Card, LoadingIndicator } from "material/react";
import { Banner, Chip, SettingList, SettingRow, toast } from "./ui";
import { errorMessage, useAsyncLoad } from "./async-state";
import { usePending } from "./use-pending";
import { useRegisterNextAction, type StepNextAction } from "./step-next-action";
import {
  getPermissionStates,
  openPermissionSettings,
  requestPermissions,
  type PermissionKey,
  type PermissionSettingsTarget,
  type PermissionState,
  type PermissionStatus,
} from "./permissions";

type CatalogEntry = {
  id: PermissionKey;
  title: string;
  description: string;
  /** 展示给用户的技术权限名，便于对照系统设置。 */
  permission: string;
  icon: ReactNode;
  settingsTarget: PermissionSettingsTarget;
};

/**
 * 需要用户确认的权限目录。id 与后端固定集合一一对应；文案只放在前端，
 * 后端不重复维护一份。
 */
const REQUESTABLE: CatalogEntry[] = [
  {
    id: "notifications",
    title: "通知",
    description:
      "课表上课与签到提醒、作业/考试截止、校园网异常和后台任务完成提示。关闭后所有提醒都会失效。",
    permission: "POST_NOTIFICATIONS",
    icon: <LuBellRing />,
    settingsTarget: "notifications",
  },
  {
    id: "promotedNotifications",
    title: "实时活动（流体云）",
    description:
      "课前 20 分钟在锁屏与状态栏显示当前课程、进度与倒计时，上课即收起。Android 16 实时更新与 ColorOS 流体云依赖它。",
    permission: "POST_PROMOTED_NOTIFICATIONS",
    icon: <LuRadio />,
    settingsTarget: "notifications",
  },
  {
    id: "location",
    title: "位置",
    description:
      "读取当前 Wi-Fi 名称，判断宿舍区/教工区并自动登录校园网；签到页的「使用当前位置」也需要它。不用于定位追踪。",
    permission: "ACCESS_FINE_LOCATION / ACCESS_COARSE_LOCATION",
    icon: <LuMapPin />,
    settingsTarget: "location",
  },
  {
    id: "nearbyWifi",
    title: "附近的设备（Wi-Fi）",
    description:
      "Android 13 起读取 Wi-Fi 名称还需要「附近的设备」权限，与位置权限一起用于校园网识别。",
    permission: "NEARBY_WIFI_DEVICES",
    icon: <LuWifi />,
    settingsTarget: "app",
  },
  {
    id: "camera",
    title: "相机",
    description: "扫描签到二维码、加入共享房间，以及在网页内拍照上传。",
    permission: "CAMERA",
    icon: <LuCamera />,
    settingsTarget: "app",
  },
  {
    id: "installPackages",
    title: "安装未知应用",
    description: "检查更新后直接安装新版本 APK，不需要再手动打开文件管理器。",
    permission: "REQUEST_INSTALL_PACKAGES",
    icon: <LuPackagePlus />,
    settingsTarget: "installPackages",
  },
  {
    id: "exactAlarm",
    title: "精确闹钟",
    description: "让课表与签到提醒在准确时间触发；不开启时系统可能延迟几分钟。",
    permission: "SCHEDULE_EXACT_ALARM",
    icon: <LuAlarmClock />,
    settingsTarget: "exactAlarm",
  },
  {
    id: "autoStart",
    title: "后台自启动",
    description:
      "可选。允许系统在开机或清理后台后恢复应用，及时收到课表、签到与待办提醒。各厂商的入口名称不同，应用无法读取是否已开启，需要在下一层系统设置中手动允许。",
    permission: "厂商自启动管理（无官方权限）",
    icon: <LuPower />,
    settingsTarget: "autoStart",
  },
  {
    id: "batteryOptimization",
    title: "电池优化白名单",
    description:
      "可选。把应用加入系统电池优化白名单，避免后台提醒被延迟或拦截。未开启时提醒仍会发送，但可能晚几分钟。",
    permission: "REQUEST_IGNORE_BATTERY_OPTIMIZATIONS",
    icon: <LuBatteryCharging />,
    settingsTarget: "batteryOptimization",
  },
];

/** 安装时由系统直接授予、不需要单独确认的普通权限。 */
const AUTOMATIC = [
  {
    title: "网络访问",
    description: "登录统一认证、同步设置、探测校园网与更新内容。",
    permission: "INTERNET / ACCESS_NETWORK_STATE / ACCESS_WIFI_STATE",
    icon: <LuGlobe />,
  },
  {
    title: "前台服务与唤醒锁",
    description: "选课、UOOC 刷课与课表实时活动的后台任务；运行时由系统显示常驻通知。",
    permission: "FOREGROUND_SERVICE / FOREGROUND_SERVICE_DATA_SYNC / WAKE_LOCK",
    icon: <LuServer />,
  },
  {
    title: "开机恢复",
    description: "设备重启后恢复课表提醒的排程，不需要重新打开应用。",
    permission: "RECEIVE_BOOT_COMPLETED",
    icon: <LuRotateCcw />,
  },
];

/**
 * 系统权限页：设置 → 权限，与初始化向导共用。
 *
 * 状态只在读取成功后渲染；读取失败时禁用申请并给出重试，不用默认值兜底。
 * 向导模式把「授权全部并继续」注册到底栏，逐项按钮仍然保留。
 */
export function PermissionSettings({
  onboarding = false,
  onComplete,
  registerNext,
}: {
  onboarding?: boolean;
  onComplete?: () => void | Promise<void>;
  registerNext?: (action: StepNextAction | null) => void;
}) {
  const pending = usePending();
  const { state, reload, apply } = useAsyncLoad(getPermissionStates);

  const byId = useMemo(() => {
    const map = new Map<PermissionKey, PermissionState>();
    if (state.status === "ready") {
      for (const item of state.data) map.set(item.id, item);
    }
    return map;
  }, [state]);

  /** 从系统设置返回时静默刷新，不把已有内容切成 loading。 */
  const refreshSilently = useCallback(async () => {
    try {
      apply(await getPermissionStates());
    } catch {
      // 刷新失败保留上一份快照；用户仍可点重试或重新进入页面。
    }
  }, [apply]);

  useEffect(() => {
    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") void refreshSilently();
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => document.removeEventListener("visibilitychange", onVisibilityChange);
  }, [refreshSilently]);

  const requestOne = async (entry: CatalogEntry) => {
    const outcome = await pending.run("perm", () => requestPermissions([entry.id]));
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("申请权限失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    apply(outcome.value);
    const next = outcome.value.find((item) => item.id === entry.id);
    if (next?.status === "granted") {
      toast.success(`${entry.title}权限已授予`);
    } else {
      toast.warning(`${entry.title}权限未开启`, {
        description: "可以稍后在系统设置中修改。",
        actionLabel: "去设置",
        onAction: () => void openSystemSettings(entry),
      });
    }
  };

  const openSystemSettings = async (entry: CatalogEntry) => {
    const outcome = await pending.run(`setting:${entry.id}`, () =>
      openPermissionSettings(entry.settingsTarget),
    );
    if (!outcome.ok && !outcome.skipped) {
      toast.danger("无法打开系统设置", { description: errorMessage(outcome.error) });
    }
  };

  const requestable =
    state.status === "ready"
      ? state.data.filter(
          (item) => item.status !== "granted" && item.status !== "unsupported" && item.canRequest,
        )
      : [];
  const hasRequestable = requestable.length > 0;

  useRegisterNextAction(
    registerNext,
    onboarding && onComplete
      ? {
          label: hasRequestable ? "授权全部并继续" : "继续",
          disabled: state.status === "idle" || state.status === "loading",
          busy: pending.isPending("perm"),
          run: async () => {
            if (hasRequestable) {
              const outcome = await pending.run("perm", () =>
                requestPermissions(requestable.map((item) => item.id)),
              );
              if (!outcome.ok) {
                throw outcome.error ?? new Error("权限申请未完成");
              }
              apply(outcome.value);
              const remaining = outcome.value.filter(
                (item) => item.status !== "granted" && item.status !== "unsupported",
              );
              if (remaining.some((item) => !item.canRequest)) {
                toast.warning("还有权限需要在系统设置中开启", {
                  description: "可以稍后在「设置 → 权限」中逐项完成。",
                });
              } else if (remaining.length > 0) {
                toast.warning("部分权限未开启", {
                  description: "可以稍后在「设置 → 权限」中重新授权。",
                });
              } else {
                toast.success("所需权限已处理");
              }
            }
            await onComplete();
          },
        }
      : null,
  );

  const renderControl = (entry: CatalogEntry) => {
    const item = byId.get(entry.id);
    return renderPermissionControl(
      item,
      {
        requestPending: pending.isPending("perm"),
        settingsPending: pending.isPending(`setting:${entry.id}`),
        onRequest: () => void requestOne(entry),
        onOpenSettings: () => void openSystemSettings(entry),
      },
      entry.title,
    );
  };

  const loading = state.status === "idle" || state.status === "loading";

  return (
    <>
      {loading && (
        <p className="status-line">
          <LoadingIndicator size={16} aria-label="正在读取系统权限" />
          正在读取系统权限…
        </p>
      )}

      {state.status === "error" && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>读取系统权限失败</Banner.Title>
            <Banner.Description>
              <span className="block">{state.message}</span>
              <span className="block">暂时无法申请或跳转系统设置；重试成功后即可继续。</span>
            </Banner.Description>
          </Banner>
          <Button variant="outlined" onClick={() => void reload()}>
            重试
          </Button>
        </div>
      )}

      {state.status === "ready" && (
        <>
          <Card variant="outlined">
            <div slot="headline">需要你授权的权限</div>
            <div slot="subhead">
              每一项只用于下面写明的功能；拒绝不会影响查看课表、查询电费等基础功能。
              部分权限在较旧的系统上会由安装过程自动授予。
            </div>
            <SettingList>
              {REQUESTABLE.map((entry) => {
                const item = byId.get(entry.id);
                return (
                  <SettingRow
                    key={entry.id}
                    icon={entry.icon}
                    title={entry.title}
                    description={
                      <>
                        {entry.description}
                        {item?.note != null && (
                          <span className="block text-muted">{item.note}</span>
                        )}
                        <span className="block break-all font-mono text-[11px] text-muted">
                          {entry.permission}
                        </span>
                      </>
                    }
                  >
                    {renderControl(entry)}
                  </SettingRow>
                );
              })}
            </SettingList>
          </Card>

          <Card variant="outlined">
            <div slot="headline">安装时自动授予</div>
            <div slot="subhead">
              这些是普通权限，安装时由系统直接授予，不需要单独确认；列出来是为了说明应用会用到什么。
            </div>
            <SettingList>
              {AUTOMATIC.map((entry) => (
                <SettingRow
                  key={entry.title}
                  icon={entry.icon}
                  title={entry.title}
                  description={
                    <>
                      {entry.description}
                      <span className="block break-all font-mono text-[11px] text-muted">
                        {entry.permission}
                      </span>
                    </>
                  }
                >
                  <Chip color="default">安装时授予</Chip>
                </SettingRow>
              ))}
            </SettingList>
          </Card>
        </>
      )}
    </>
  );
}

function statusLabel(status: PermissionStatus): {
  chip: string;
  color: "success" | "warning" | "default";
} {
  switch (status) {
    case "granted":
      return { chip: "已授予", color: "success" };
    case "unsupported":
      return { chip: "系统不支持", color: "default" };
    case "systemDisabled":
      return { chip: "系统关闭", color: "warning" };
    case "manual":
      return { chip: "需手动确认", color: "warning" };
    case "unknown":
      return { chip: "未知", color: "default" };
    case "denied":
      return { chip: "未授予", color: "warning" };
  }
}

function renderPermissionControl(
  item: PermissionState | undefined,
  handlers: {
    requestPending: boolean;
    settingsPending: boolean;
    onRequest: () => void;
    onOpenSettings: () => void;
  },
  title: string,
) {
  const status = item?.status ?? "unknown";
  const { chip, color } = statusLabel(status);
  if (status === "granted" || status === "unsupported" || status === "unknown") {
    return <Chip color={color}>{chip}</Chip>;
  }
  const canRequest = item?.canRequest === true;
  const action = canRequest ? "授权" : "去设置";
  return (
    <div className="flex items-center gap-2">
      <Chip color={color}>{chip}</Chip>
      <Button
        variant={canRequest ? "filled" : "outlined"}
        disabled={canRequest ? handlers.requestPending : handlers.settingsPending}
        onClick={canRequest ? handlers.onRequest : handlers.onOpenSettings}
        aria-label={`${action}${title}`}
      >
        {action}
      </Button>
    </div>
  );
}
