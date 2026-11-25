import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, List, ListItem, LoadingIndicator, Switch, TextField } from "material/react";
import { Banner, Chip, SettingList, SettingRow, bindField, toast } from "./ui";
import { errorMessage } from "./async-state";
import { platformLabel } from "./platform-label";
import { PairAuthorizeEntry } from "./pair-login/AuthorizeEntry";
import { usePending } from "./use-pending";
import { LuRefreshCw } from "react-icons/lu";

export type MeshStatus = {
  enabled: boolean;
  campusAccess: boolean;
  running: boolean;
  deviceId: string;
  deviceName: string;
  overlayIp: string | null;
  campusEgress: string | null;
  apiPort: number | null;
  error: string | null;
};

export type MeshDevice = {
  deviceId: string;
  name: string;
  platform: string;
  appVersion: string;
  overlayIp: string | null;
  isSelf: boolean;
  reachable: boolean;
  latencyMs: number | null;
  syncEnabled: boolean;
  campusAccess: boolean;
  onCampus: boolean;
  region: string;
  networkState: string;
  ssid: string | null;
};

export type SyncItem = {
  id: string;
  label: string;
  collection: boolean;
  state: "synced" | "waiting" | "disabled" | "loggedOut";
};

export type SyncSummary = {
  peers: number;
  applied: number;
  pushed: number;
  bankRows: number;
  errors: string[];
};

export type SyncStatus = {
  enabled: boolean;
  campusAccess: boolean;
  running: boolean;
  summary: SyncSummary;
  items: SyncItem[];
};

export default function SettingsSync() {
  const pending = usePending();
  const [status, setStatus] = useState<MeshStatus | null>(null);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [devices, setDevices] = useState<MeshDevice[]>([]);
  const [nameDraft, setNameDraft] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const applyStatus = useCallback((next: MeshStatus) => {
    setStatus(next);
    setNameDraft((current) => (current === "" ? next.deviceName : current));
  }, []);

  const reload = useCallback(async () => {
    const [nextStatus, nextDevices, nextSync] = await Promise.all([
      invoke<MeshStatus>("get_mesh_status"),
      invoke<MeshDevice[]>("list_mesh_devices"),
      invoke<SyncStatus>("get_sync_status"),
    ]);
    applyStatus(nextStatus);
    setDevices(nextDevices);
    setSync(nextSync);
  }, [applyStatus]);

  /** 读取失败是持久状态，用横幅加重试；失败时开关不可用，避免在未知状态下切换。 */
  const retryLoad = async () => {
    const result = await pending.run("reload", reload);
    if (!result.ok) {
      if (!result.skipped) setLoadError(errorMessage(result.error, ""));
      return;
    }
    setLoadError(null);
  };

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const subscribe = async () => {
      unlisten.push(
        await listen<MeshStatus>("mesh-status-changed", ({ payload }) => {
          if (!disposed) applyStatus(payload);
        }),
      );
      unlisten.push(
        await listen<MeshDevice[]>("mesh-devices-changed", ({ payload }) => {
          if (!disposed) setDevices(payload);
        }),
      );
      unlisten.push(
        await listen<SyncStatus>("sync-status-changed", ({ payload }) => {
          if (!disposed) setSync(payload);
        }),
      );
      try {
        await reload();
        if (!disposed) setLoadError(null);
      } catch (error) {
        if (!disposed) setLoadError(errorMessage(error, ""));
      } finally {
        if (!disposed) setLoaded(true);
      }
    };
    void subscribe();
    return () => {
      disposed = true;
      for (const stop of unlisten) stop();
    };
  }, [applyStatus, reload]);

  const toggleEnabled = async (enabled: boolean) => {
    const result = await pending.run("toggle", () =>
      invoke<MeshStatus>("set_mesh_enabled", { enabled }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("切换设备同步失败", { description: errorMessage(result.error, "") });
      return;
    }
    applyStatus(result.value);
    if (enabled) {
      toast.success("已开启设备同步", {
        description: result.value.running ? "正在与同一账号的其它设备建立连接。" : "网络启动中。",
      });
    } else {
      toast.info("已关闭设备同步");
    }
  };

  const runSync = async () => {
    const result = await pending.run("sync", () => invoke<SyncStatus>("trigger_sync"));
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("同步失败", { description: errorMessage(result.error, "") });
      return;
    }
    const next = result.value;
    setSync(next);
    if (next.summary.errors.length > 0) {
      toast.warning("同步完成，但有对端失败", {
        description: next.summary.errors.join("；"),
      });
    } else if (next.summary.peers === 0) {
      toast.info("暂未发现其它设备", { description: "请确认另一台设备已登录并开启同步。" });
    } else {
      toast.success("同步完成", {
        description: `拉取 ${next.summary.applied} 项，推送 ${next.summary.pushed} 项，题库 ${next.summary.bankRows} 行。`,
      });
    }
  };

  /** 名称在失焦/回车时提交，没有独立的保存按钮。 */
  const commitName = async (value: string) => {
    const name = value.trim();
    if (!loaded) return;
    if (name === "" || name === status?.deviceName) {
      setNameDraft(status?.deviceName ?? "");
      return;
    }
    const result = await pending.run("rename", () =>
      invoke<MeshStatus>("set_mesh_device_name", { name }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("名称保存失败", { description: errorMessage(result.error, "") });
      return;
    }
    applyStatus(result.value);
    setNameDraft(result.value.deviceName);
    toast.success("设备名称已保存");
  };

  const restoreItem = async (id: string) => {
    const result = await pending.run(`item:${id}`, () =>
      invoke<SyncStatus>("set_sync_item_enabled", { id, enabled: true }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("恢复同步失败", { description: errorMessage(result.error, "") });
      return;
    }
    setSync(result.value);
    toast.success("已恢复该项同步");
  };

  const peerCount = devices.filter((device) => !device.isSelf).length;
  const onlineCount = devices.filter((device) => !device.isSelf && device.reachable).length;
  const disabledItems = (sync?.items ?? []).filter((item) => item.state === "disabled");
  const toggling = pending.isPending("toggle");
  const syncing = pending.isPending("sync");

  return (
    <>
      {loadError != null && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>读取设备同步状态失败</Banner.Title>
            <Banner.Description>{loadError}</Banner.Description>
          </Banner>
          <Button
            variant="outlined"
            disabled={pending.isPending("reload")}
            onClick={() => void retryLoad()}
          >
            重试
          </Button>
        </div>
      )}

      {status?.error && (
        <Banner status="danger">
          <Banner.Title>设备同步网络未运行</Banner.Title>
          <Banner.Description>{status.error}</Banner.Description>
        </Banner>
      )}

      <Card variant="outlined">
        <div slot="headline">设备同步</div>
        <div slot="subhead">
          同一账号的设备会自动组网，同步登录态、题库与已配置的账号密码。两台设备同时改同一项时，后保存的覆盖先保存的。
        </div>
        <SettingList>
          <SettingRow
            icon={<LuRefreshCw aria-hidden="true" />}
            title="开启设备同步"
            description="关闭时不监听任何端口，也不会处理对端请求。"
          >
            <Switch
              checked={Boolean(status?.enabled)}
              disabled={!loaded || loadError != null || toggling}
              onChange={(event: any) =>
                void toggleEnabled(Boolean(event.detail?.checked ?? event.target?.checked))
              }
              aria-label="启用设备同步"
            />
          </SettingRow>
          <SettingRow title="网络状态">
            <span className="flex items-center gap-2">
              {!status?.enabled ? (
                <Chip color="default">未启用</Chip>
              ) : status?.running ? (
                <Chip color="success">已连接</Chip>
              ) : (
                <Chip color="warning">启动中</Chip>
              )}
              {toggling && <LoadingIndicator size={16} aria-label="正在切换设备同步" />}
            </span>
          </SettingRow>
          <SettingRow title="已发现设备">
            <span className="text-sm">
              {peerCount === 0 ? "暂无其它设备" : `${onlineCount} / ${peerCount} 台在线`}
            </span>
          </SettingRow>
          <SettingRow title="本机名称" description="只用于设备列表展示，不影响网络身份。">
            <TextField
              // Material 的 TextField 自带 280px 固有宽度，比 192px 更接近它的常态排版。
              className="w-72"
              label="设备名称"
              value={nameDraft}
              disabled={!loaded || loadError != null || pending.isPending("rename")}
              {...bindField({ input: setNameDraft, change: (value) => void commitName(value) })}
            />
          </SettingRow>
          <SettingRow title="立即同步" description="立即与其它在线设备交换一次设置。">
            <Button
              variant="tonal"
              disabled={!status?.enabled || !loaded || syncing}
              onClick={() => void runSync()}
            >
              {syncing ? (
                <LoadingIndicator size={16} />
              ) : (
                <LuRefreshCw className="size-4" aria-hidden="true" />
              )}
              立即同步
            </Button>
          </SettingRow>
        </SettingList>
      </Card>

      <Card variant="outlined">
        <div slot="headline">网络内的设备</div>
        <div slot="subhead">列表只显示同一账号的设备。</div>
        {devices.length === 0 ? (
          <p className="text-sm text-muted">
            {status?.enabled ? "正在等待其它设备加入…" : "开启设备同步后，这里会显示同账号的设备。"}
          </p>
        ) : (
          <List aria-label="同步网络内的设备">
            {devices.map((device) => (
              <ListItem
                key={device.deviceId || device.overlayIp || device.name}
                value={device.deviceId || device.overlayIp || device.name}
                headline={device.name}
                overline={
                  device.isSelf
                    ? "本机"
                    : `${platformLabel(device.platform)}${device.appVersion ? ` · ${device.appVersion}` : ""}`
                }
                supportingText={
                  device.overlayIp
                    ? `${device.overlayIp}${device.latencyMs ? ` · ${device.latencyMs} ms` : ""}`
                    : "等待分配虚拟 IP"
                }
                trailingText={device.isSelf ? "本机" : device.reachable ? "在线" : "离线"}
              />
            ))}
          </List>
        )}
      </Card>

      <PairAuthorizeEntry />

      <Card variant="outlined">
        <div slot="headline">同步内容</div>
        <div slot="subhead">
          只同步下列白名单。账号资料、统一认证会话、畅课/巡课登录态与本地外观、通知设置不会同步。
        </div>
        <List aria-label="同步项状态">
          {(sync?.items ?? []).map((item) => (
            <ListItem
              key={item.id}
              value={item.id}
              headline={item.label}
              supportingText={
                item.state === "loggedOut"
                  ? "全局退出标记"
                  : item.state === "disabled"
                    ? "本机已停用"
                    : item.collection
                      ? "逐题同步"
                      : "整项同步"
              }
              trailingText={
                item.state === "disabled"
                  ? "已停用"
                  : item.state === "loggedOut"
                    ? "已全局退出"
                    : item.state === "synced"
                      ? "已同步"
                      : "等待对端"
              }
            />
          ))}
        </List>
        {disabledItems.length > 0 && (
          <SettingList className="mt-2">
            {disabledItems.map((item) => {
              const key = `item:${item.id}`;
              const restoring = pending.isPending(key);
              return (
                <SettingRow
                  key={item.id}
                  title={`${item.label} 已停止同步`}
                  description="恢复后重新参与同步。"
                >
                  <Button
                    variant="tonal"
                    disabled={restoring}
                    onClick={() => void restoreItem(item.id)}
                  >
                    {restoring && <LoadingIndicator size={16} />}
                    恢复同步
                  </Button>
                </SettingRow>
              );
            })}
          </SettingList>
        )}
      </Card>

      {status?.enabled && peerCount === 0 && (
        <Banner status="info">
          <Banner.Title>还没有发现其它设备</Banner.Title>
          <Banner.Description>
            在另一台设备登录同一账号并开启同步后，两台设备会自动建立连接。若长时间没有出现，通常是预置中继节点暂时不可达。
          </Banner.Description>
        </Banner>
      )}
    </>
  );
}
