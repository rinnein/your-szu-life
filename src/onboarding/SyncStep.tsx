import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, Switch } from "material/react";
import { Banner, toast } from "../ui";
import { useRegisterNextAction } from "../step-next-action";
import type { SyncStatus } from "../SettingsSync";
import type { StepProps } from "./step-types";

type MeshStatus = { enabled: boolean; running: boolean; error: string | null };

export function SyncStep({ onComplete, registerNext }: StepProps) {
  const [enabled, setEnabled] = useState(true);
  const [busy, setBusy] = useState(false);
  const [started, setStarted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState<SyncStatus["summary"] | null>(null);

  useEffect(() => {
    let active = true;
    void invoke<MeshStatus>("get_mesh_status")
      .then((status) => {
        if (active && status.enabled) setEnabled(true);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  useRegisterNextAction(registerNext, {
    label: enabled ? (summary ? "下一步" : "开启并立即同步") : "不开启，继续",
    busy,
    run: async () => {
      setBusy(true);
      setError(null);
      try {
        if (enabled) {
          const status = await invoke<MeshStatus>("set_mesh_enabled", { enabled: true });
          if (!status.enabled) throw new Error(status.error ?? "设备同步网络未能启动");
          const next = await invoke<SyncStatus>("trigger_sync");
          setSummary(next.summary);
          setStarted(true);
          if (next.summary.peers === 0) {
            toast.info("暂未发现其它设备", {
              description: "先继续完成初始化，之后在「设置 - 同步」里随时可以再次同步。",
            });
          } else {
            toast.success("已同步其它设备上的配置", {
              description: `拉取 ${next.summary.applied} 项、题库 ${next.summary.bankRows} 行。`,
            });
          }
        }
      } catch (cause) {
        const message =
          typeof cause === "string" ? cause : cause instanceof Error ? cause.message : "";
        setError(message || "自动同步暂未完成");
        toast.warning("自动同步暂未完成", { description: "可以先继续初始化，稍后在设置里重试。" });
      } finally {
        setBusy(false);
      }
      await onComplete();
    },
  });

  return (
    <Card variant="outlined">
      <div slot="headline">自动同步</div>
      <div slot="subhead">
        同一账号的设备会自动组网。开启后，可以先把其它设备上已经配置好的 UOOC 登录态、题库、OJ
        与校园网账号、宿舍信息和 AI 配置同步过来，后面的设置步骤就不用重复填写。
      </div>

      <div className="mt-5 flex items-center justify-between gap-4">
        <div className="space-y-1">
          <div className="text-sm font-medium text-foreground">开启自动同步</div>
          <p className="text-xs text-muted-foreground">
            只同步上面这些内容；账号资料、统一认证会话与外观/通知设置不会离开本机。
          </p>
        </div>
        <Switch
          checked={enabled}
          disabled={busy}
          onChange={(event: any) => {
            setEnabled(Boolean(event.detail?.checked ?? event.target?.checked));
          }}
          aria-label="开启自动同步"
        />
      </div>

      {busy && (
        <p className="mt-4 flex items-center gap-2 text-sm text-muted-foreground">
          <LoadingIndicator size={16} aria-label="正在同步" />
          正在寻找其它设备并同步，最多需要十几秒…
        </p>
      )}

      {started && summary && (
        <Banner status={summary.peers === 0 ? "info" : "success"} className="mt-4">
          <Banner.Title>
            {summary.peers === 0 ? "已开启同步，但还没有发现其它设备" : "同步完成"}
          </Banner.Title>
          <Banner.Description>
            {summary.peers === 0
              ? "在另一台设备登录同一账号并开启同步后，两台设备会自动建立连接。"
              : `对端 ${summary.peers} 台，拉取 ${summary.applied} 项、推送 ${summary.pushed} 项、题库 ${summary.bankRows} 行。`}
          </Banner.Description>
        </Banner>
      )}

      {error && (
        <Banner status="warning" className="mt-4">
          <Banner.Title>自动同步没有完成</Banner.Title>
          <Banner.Description>{error}。可以先继续，稍后在「设置 - 同步」重试。</Banner.Description>
        </Banner>
      )}
    </Card>
  );
}
