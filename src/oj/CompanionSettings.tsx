import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Switch, TextField, Button } from "material/react";
import { Chip, bindField } from "../ui";
import type { Snapshot } from "./types";
export function CompanionSettings({
  snapshot,
  onError,
}: {
  snapshot: Snapshot;
  onError: (message: string) => void;
}) {
  const [port, setPort] = useState(String(snapshot.companionPort));
  const [busy, setBusy] = useState(false);
  useEffect(() => setPort(String(snapshot.companionPort)), [snapshot.companionPort]);
  if (!snapshot.desktop) return null;
  const status = snapshot.companionStatus;
  const apply = async (enabled: boolean) => {
    setBusy(true);
    try {
      const number = Number(port);
      if (!Number.isInteger(number) || number < 1 || number > 65535)
        throw new Error("请输入有效的 cph-ng 本机端口");
      await invoke("configure_oj_companion", { enabled, port: number });
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card variant="outlined">
      <div slot="headline">cph-ng 直接提交</div>
      <div slot="subhead">
        此处配置适用于全部 OJ。发送题目和 cph-ng 直接提交共用同一个本机端口；编译选项仍分别保存在各
        OJ 实例中。
      </div>
      <div className="oj-form p-4">
        <div className="oj-actions">
          <Switch
            checked={snapshot.companionEnabled ?? true}
            disabled={busy}
            onChange={(e: any) => void apply(e.detail?.checked ?? e.target?.checked)}
          >
            接受 cph-ng 提交
          </Switch>
          <Chip color={status?.active ? "success" : "default"}>
            {status?.active ? "当前提交端" : status?.connected ? "已连接" : "未连接"}
          </Chip>
        </div>
        <div className="oj-actions">
          <TextField
            label="本机接收端口"
            value={port}
            {...bindField(setPort)}
            placeholder="默认 27121，与 cph-ng 保持一致"
          />
          <Button
            variant="tonal"
            disabled={busy}
            onClick={() => void apply(snapshot.companionEnabled ?? true)}
          >
            保存连接设置
          </Button>
          {status?.connected && !status.active && (
            <Button
              onClick={() => void invoke("activate_oj_companion").catch((e) => onError(String(e)))}
            >
              使用泥深提交
            </Button>
          )}
        </div>
        <p role="status">{status?.message}</p>
      </div>
    </Card>
  );
}
