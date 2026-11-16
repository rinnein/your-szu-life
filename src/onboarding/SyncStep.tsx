import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Switch } from "material/react";
import { Banner, SettingRow, toast } from "../ui";
import { errorMessage } from "../async-state";
import { usePending } from "../use-pending";
import { useRegisterNextAction } from "../step-next-action";
import type { StepProps } from "./step-types";

type MeshStatus = { enabled: boolean; running: boolean; error: string | null };

/** 同步结果的摘要；只取这一步展示需要的字段，不依赖设置页的类型。 */
type SyncSummary = {
  peers: number;
  applied: number;
  pushed: number;
  bankRows: number;
  errors: string[];
};

/** 一次同步尝试的结果。空闲、已有结果、失败三态互斥，底栏文案由它派生。 */
type SyncOutcome =
  | { status: "idle" }
  | { status: "done"; summary: SyncSummary }
  | { status: "failed"; message: string };

export function SyncStep({ onComplete, registerNext }: StepProps) {
  // 默认替用户勾上：这一步问的是「要不要现在同步」，不是「把已有开关读出来」。
  const [enabled, setEnabled] = useState(true);
  const [outcome, setOutcome] = useState<SyncOutcome>({ status: "idle" });
  const pending = usePending();
  const syncing = pending.isPending("sync");

  useRegisterNextAction(registerNext, {
    label: enabled ? (outcome.status === "done" ? "下一步" : "开启并立即同步") : "不开启，继续",
    busy: syncing,
    run: async () => {
      if (enabled) {
        const attempt = await pending.run("sync", async () => {
          const status = await invoke<MeshStatus>("set_mesh_enabled", { enabled: true });
          if (!status.enabled) throw new Error(status.error ?? "设备同步网络未能启动");
          return invoke<{ summary: SyncSummary }>("trigger_sync");
        });
        if (attempt.ok) {
          const { summary } = attempt.value;
          setOutcome({ status: "done", summary });
          if (summary.peers === 0) {
            toast.info("暂未发现其它设备", {
              description: "先继续完成初始化，之后在「设置 - 同步」里随时可以再次同步。",
            });
          } else {
            toast.success("已同步其它设备上的配置", {
              description: `拉取 ${summary.applied} 项、题库 ${summary.bankRows} 行。`,
            });
          }
        } else if (!attempt.skipped) {
          setOutcome({
            status: "failed",
            message: errorMessage(attempt.error, "自动同步暂未完成"),
          });
          toast.warning("自动同步暂未完成", {
            description: "可以先继续初始化，稍后在设置里重试。",
          });
        }
      }
      // 同步失败不阻塞初始化：这一步只负责「能同步就同步」。
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

      <SettingRow
        title="开启自动同步"
        description="只同步上面这些内容；账号资料、统一认证会话与外观、通知设置不会离开本机。"
      >
        <Switch
          checked={enabled}
          disabled={syncing}
          onChange={(event: any) => {
            setEnabled(Boolean(event.detail?.checked ?? event.target?.checked));
          }}
          aria-label="开启自动同步"
        />
      </SettingRow>

      {syncing && (
        <p className="status-line" role="status">
          正在寻找其它设备并同步，最多需要十几秒…
        </p>
      )}

      {outcome.status === "done" && (
        <Banner status={outcome.summary.peers === 0 ? "info" : "success"}>
          <Banner.Title>
            {outcome.summary.peers === 0 ? "已开启同步，但还没有发现其它设备" : "同步完成"}
          </Banner.Title>
          <Banner.Description>
            {outcome.summary.peers === 0
              ? "在另一台设备登录同一账号并开启同步后，两台设备会自动建立连接。"
              : `对端 ${outcome.summary.peers} 台，拉取 ${outcome.summary.applied} 项、推送 ${outcome.summary.pushed} 项、题库 ${outcome.summary.bankRows} 行。`}
          </Banner.Description>
        </Banner>
      )}

      {outcome.status === "failed" && (
        <Banner status="warning">
          <Banner.Title>自动同步没有完成</Banner.Title>
          <Banner.Description>
            {outcome.message}。可以先继续，稍后在「设置 - 同步」重试。
          </Banner.Description>
        </Banner>
      )}
    </Card>
  );
}
