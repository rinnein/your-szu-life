import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  LuBell,
  LuBookOpen,
  LuCode,
  LuGraduationCap,
  LuShieldAlert,
  LuWifi,
  LuZap,
} from "react-icons/lu";
import { Button, Card, Switch } from "material/react";
import { Banner, toast } from "./ui";

export interface NotificationConfig {
  enabled: boolean;
  campusNetwork: boolean;
  electricity: boolean;
  enrollment: boolean;
  oj: boolean;
  uooc: boolean;
}

const DEFAULT_CONFIG: NotificationConfig = {
  enabled: true,
  campusNetwork: true,
  electricity: true,
  enrollment: true,
  oj: true,
  uooc: true,
};

export function NotificationSettings() {
  const [config, setConfig] = useState<NotificationConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(true);
  const [permissionGranted, setPermissionGranted] = useState<boolean | null>(null);
  const [requesting, setRequesting] = useState(false);

  useEffect(() => {
    let active = true;
    void invoke<NotificationConfig>("get_notification_settings")
      .then((saved) => {
        if (active && saved) setConfig(saved);
      })
      .catch(() => {
        // 使用默认值
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    const checkPermission = async () => {
      try {
        const { isPermissionGranted } = await import("@tauri-apps/plugin-notification");
        const granted = await isPermissionGranted();
        if (active) setPermissionGranted(granted);
      } catch {
        if (active) setPermissionGranted(true);
      }
    };
    void checkPermission();

    return () => {
      active = false;
    };
  }, []);

  const handleRequestPermission = async () => {
    setRequesting(true);
    try {
      const { isPermissionGranted, requestPermission } =
        await import("@tauri-apps/plugin-notification");
      let granted = await isPermissionGranted();
      if (!granted) {
        const permission = await requestPermission();
        granted = permission === "granted";
      }
      setPermissionGranted(granted);
      if (granted) {
        toast.success("已获得系统通知权限");
      } else {
        toast.warning("系统通知权限未开启", {
          description: "请在系统设置中允许本应用发送通知。",
        });
      }
    } catch (e) {
      toast.danger("申请通知权限失败", {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setRequesting(false);
    }
  };

  const updateConfig = (updater: (prev: NotificationConfig) => NotificationConfig) => {
    setConfig((prev) => {
      const next = updater(prev);
      void invoke("save_notification_settings", { settings: next }).catch((e) => {
        toast.danger("保存通知设置失败", {
          description: e instanceof Error ? e.message : String(e),
        });
      });
      return next;
    });
  };

  const categories = [
    {
      key: "campusNetwork" as const,
      title: "宿舍校园网",
      description: "自动登录失败或断线重连异常时发送通知",
      icon: LuWifi,
    },
    {
      key: "electricity" as const,
      title: "宿舍电费预警",
      description: "宿舍电量偏低或预计可用天数不足时发送充值预警",
      icon: LuZap,
    },
    {
      key: "enrollment" as const,
      title: "选课助手",
      description: "抢课任务完成、暂停或需要 2FA 二次验证时发送通知",
      icon: LuGraduationCap,
    },
    {
      key: "oj" as const,
      title: "在线评测 (OJ)",
      description: "新测验发布及测验临近截止（1小时前）提醒",
      icon: LuCode,
    },
    {
      key: "uooc" as const,
      title: "优课联盟 (UOOC)",
      description: "课程测验、作业待处理或临近截止时发送提醒",
      icon: LuBookOpen,
    },
  ];

  return (
    <div className="notification-settings space-y-6 max-w-3xl">
      {permissionGranted === false && (
        <Banner status="warning" className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-2">
            <LuShieldAlert className="size-5 shrink-0" />
            <span>
              系统通知权限未开启。在后台运行时将无法接收系统推送通知；前台聚焦使用时仍会通过应用内
              Snackbar 提示。
            </span>
          </div>
          <Button
            variant="text"
            disabled={requesting}
            onClick={() => void handleRequestPermission()}
          >
            授权系统通知
          </Button>
        </Banner>
      )}

      {/* 全局总开关 */}
      <Card variant="outlined" className="p-4">
        <div className="flex items-center justify-between gap-4">
          <div className="space-y-1">
            <div className="flex items-center gap-2 font-medium text-base text-foreground">
              <LuBell className="size-5 text-primary" />
              <span>通知总开关</span>
            </div>
            <p className="text-sm text-muted-foreground">
              关闭后，所有系统通知和前台应用内提示均被静音。
            </p>
          </div>
          <Switch
            checked={config.enabled}
            disabled={loading}
            onChange={(e: any) => {
              const checked = Boolean(e.detail?.checked ?? e.target?.checked);
              updateConfig((prev) => ({ ...prev, enabled: checked }));
            }}
            aria-label="启用所有通知"
          />
        </div>
      </Card>

      {/* 业务分类开关列表 */}
      <Card variant="outlined" className="p-4 space-y-4">
        <div slot="headline" className="font-medium text-base text-foreground">
          通知分类管理
        </div>
        <p className="text-sm text-muted-foreground">
          根据需要开启或关闭特定业务的通知提醒。应用在前台聚焦状态下将自动以 Snackbar
          展示，后台运行时弹出系统通知。
        </p>

        <div className="divide-y divide-border/40">
          {categories.map((cat) => {
            const Icon = cat.icon;
            const isChecked = config[cat.key];
            const isDisabled = loading || !config.enabled;
            return (
              <div
                key={cat.key}
                className="flex items-center justify-between gap-4 py-3.5 first:pt-1 last:pb-1"
              >
                <div className="flex items-start gap-3">
                  <div className="p-2 rounded-lg bg-surface-container text-primary shrink-0 mt-0.5">
                    <Icon className="size-5" />
                  </div>
                  <div className="space-y-0.5">
                    <div className="font-medium text-sm text-foreground">{cat.title}</div>
                    <p className="text-xs text-muted-foreground">{cat.description}</p>
                  </div>
                </div>
                <Switch
                  checked={isChecked}
                  disabled={isDisabled}
                  onChange={(e: any) => {
                    const checked = Boolean(e.detail?.checked ?? e.target?.checked);
                    updateConfig((prev) => ({ ...prev, [cat.key]: checked }));
                  }}
                  aria-label={`开关${cat.title}通知`}
                />
              </div>
            );
          })}
        </div>
      </Card>
    </div>
  );
}
