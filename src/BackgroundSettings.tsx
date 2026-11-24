import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LuMonitorPlay, LuPower } from "react-icons/lu";
import { Button, Card, LoadingIndicator, Switch } from "material/react";
import { Banner, SettingList, SettingRow, toast } from "./ui";
import { errorMessage, useAsyncLoad } from "./async-state";
import { usePending } from "./use-pending";

/** 与 `szu_core::startup::StartupSettings` 对齐。 */
export interface StartupSettings {
  autostart: boolean;
  silent: boolean;
  /** 开发构建与移动端为 false：不会真正写入系统自启项。 */
  supported: boolean;
}

const DEFAULT_VIEW: StartupSettings = { autostart: true, silent: false, supported: true };

function checkedOf(event: any): boolean {
  return Boolean(event.detail?.checked ?? event.target?.checked);
}

/**
 * 设置 → 后台（仅桌面端）。
 *
 * 偏好由 `settings.sqlite` 保存，真正的系统自启项由 tauri-plugin-autostart
 * 维护。保存失败时后端不落盘，这里回滚开关并提示，避免出现「界面显示开、
 * 系统里却没有条目」。
 *
 * 「静默启动」只影响由自启拉起的那一次；关闭窗口收纳托盘的行为始终不变。
 */
export function BackgroundSettings() {
  const pending = usePending();
  const { state, reload } = useAsyncLoad(() => invoke<StartupSettings>("get_startup_settings"));
  const [settings, setSettings] = useState<StartupSettings | null>(null);
  /** 最后一次确认落盘的设置，用于失败回滚。 */
  const confirmed = useRef<StartupSettings | null>(null);

  useEffect(() => {
    if (state.status !== "ready") return;
    confirmed.current = state.data;
    setSettings(state.data);
  }, [state]);

  const loading = state.status === "idle" || state.status === "loading";
  const saving = pending.isPending("startup");
  const view = settings ?? DEFAULT_VIEW;
  const readOnly = loading || settings === null || !view.supported;

  const persist = async (patch: Partial<Pick<StartupSettings, "autostart" | "silent">>) => {
    if (!settings || readOnly || saving) return;
    const previous = confirmed.current ?? settings;
    const next = { ...settings, ...patch };
    setSettings(next);
    const outcome = await pending.run("startup", () =>
      invoke<StartupSettings>("save_startup_settings", {
        settings: { autostart: next.autostart, silent: next.silent, supported: next.supported },
      }),
    );
    if (!outcome.ok) {
      if (outcome.skipped) return;
      setSettings(previous);
      toast.danger("保存后台设置失败", { description: errorMessage(outcome.error) });
      return;
    }
    confirmed.current = outcome.value;
    setSettings(outcome.value);
  };

  return (
    <>
      {loading && (
        <p className="status-line">
          <LoadingIndicator size={16} aria-label="正在读取后台设置" />
          正在读取后台设置…
        </p>
      )}

      {state.status === "error" && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>读取后台设置失败</Banner.Title>
            <Banner.Description>
              <span className="block">{state.message}</span>
              <span className="block">
                为避免把默认值写回，开关暂时禁用；重试成功前不会保存任何改动。
              </span>
            </Banner.Description>
          </Banner>
          <Button variant="outlined" onClick={() => void reload()}>
            重试
          </Button>
        </div>
      )}

      {settings !== null && !settings.supported && (
        <Banner status="info">
          <Banner.Title>当前构建不管理开机自启</Banner.Title>
          <Banner.Description>
            开发构建与移动端不会写入系统自启项；通过安装包安装的桌面版才会生效。
          </Banner.Description>
        </Banner>
      )}

      <Card variant="outlined">
        <div slot="headline">启动与后台</div>
        <div slot="subhead">
          登录系统后自动启动应用，并可选择静默驻留托盘。关闭窗口本来就会收纳到托盘，
          提醒与后台任务继续运行。
        </div>
        <SettingList>
          <SettingRow
            icon={<LuPower aria-hidden="true" />}
            title="开机自启"
            description="登录系统后自动启动应用，会写入系统的登录自启项。"
          >
            <Switch
              checked={view.autostart}
              disabled={readOnly || saving}
              onChange={(event: any) => void persist({ autostart: checkedOf(event) })}
              aria-label="开机自启"
            />
          </SettingRow>
          <SettingRow
            icon={<LuMonitorPlay aria-hidden="true" />}
            title="静默启动"
            description="仅在由开机自启拉起时生效：不显示主窗口，直接驻留托盘，通知照常发送。手动打开应用始终显示窗口。"
          >
            <Switch
              checked={view.silent}
              disabled={readOnly || saving || !view.autostart}
              onChange={(event: any) => void persist({ silent: checkedOf(event) })}
              aria-label="静默启动"
            />
          </SettingRow>
        </SettingList>
      </Card>
    </>
  );
}
