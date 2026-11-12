import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, List, ListItem, LoadingIndicator, Switch, TextField } from "material/react";
import { Banner, Chip, bindField, toast } from "./ui";
import { LuCheck, LuRefreshCw, LuWifi } from "react-icons/lu";

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

function errorMessage(error: unknown, fallback: string) {
  return typeof error === "string" ? error : error instanceof Error ? error.message : fallback;
}

function platformLabel(platform: string) {
  switch (platform) {
    case "android":
      return "Android";
    case "windows":
      return "Windows";
    case "macos":
      return "macOS";
    case "linux":
      return "Linux";
    case "ios":
      return "iOS";
    default:
      return platform || "未知平台";
  }
}

export default function SettingsSync() {
  const [status, setStatus] = useState<MeshStatus | null>(null);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [devices, setDevices] = useState<MeshDevice[]>([]);
  const [nameDraft, setNameDraft] = useState("");
  const [toggling, setToggling] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [savingName, setSavingName] = useState(false);
  const [enablingItem, setEnablingItem] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const applyStatus = useCallback((next: MeshStatus) => {
    setStatus(next);
    setNameDraft((current) => (current === "" ? next.deviceName : current));
  }, []);

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
        const [nextStatus, nextDevices, nextSync] = await Promise.all([
          invoke<MeshStatus>("get_mesh_status"),
          invoke<MeshDevice[]>("list_mesh_devices"),
          invoke<SyncStatus>("get_sync_status"),
        ]);
        if (disposed) return;
        applyStatus(nextStatus);
        setDevices(nextDevices);
        setSync(nextSync);
      } catch (error) {
        if (!disposed)
          toast.danger("读取设备同步状态失败", { description: errorMessage(error, "") });
      } finally {
        if (!disposed) setLoaded(true);
      }
    };
    void subscribe();
    return () => {
      disposed = true;
      for (const stop of unlisten) stop();
    };
  }, [applyStatus]);

  const toggleEnabled = async (enabled: boolean) => {
    setToggling(true);
    try {
      const next = await invoke<MeshStatus>("set_mesh_enabled", { enabled });
      applyStatus(next);
      if (enabled) {
        toast.success("已开启设备同步", {
          description: next.running ? "正在与同一账号的其它设备建立连接。" : "网络启动中。",
        });
      } else {
        toast.info("已关闭设备同步");
      }
    } catch (error) {
      toast.danger("切换设备同步失败", { description: errorMessage(error, "") });
    } finally {
      setToggling(false);
    }
  };

  const runSync = async () => {
    setSyncing(true);
    try {
      const next = await invoke<SyncStatus>("trigger_sync");
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
    } catch (error) {
      toast.danger("同步失败", { description: errorMessage(error, "") });
    } finally {
      setSyncing(false);
    }
  };

  const saveName = async () => {
    setSavingName(true);
    try {
      const next = await invoke<MeshStatus>("set_mesh_device_name", { name: nameDraft });
      applyStatus(next);
      toast.success("设备名称已保存");
    } catch (error) {
      toast.danger("保存设备名称失败", { description: errorMessage(error, "") });
    } finally {
      setSavingName(false);
    }
  };

  const enableItem = async (id: string) => {
    setEnablingItem(id);
    try {
      const next = await invoke<SyncStatus>("set_sync_item_enabled", { id, enabled: true });
      setSync(next);
      toast.success("已恢复该项同步");
    } catch (error) {
      toast.danger("恢复同步失败", { description: errorMessage(error, "") });
    } finally {
      setEnablingItem(null);
    }
  };

  const peerCount = devices.filter((device) => !device.isSelf).length;
  const onlineCount = devices.filter((device) => !device.isSelf && device.reachable).length;
  const disabledItems = (sync?.items ?? []).filter((item) => item.state === "disabled");

  return (
    <section aria-labelledby="settings-sync-title" className="grid gap-4">
      <header className="max-w-2xl">
        <h2 id="settings-sync-title" className="text-xl font-semibold tracking-tight">
          同步
        </h2>
        <p className="mt-1.5 text-sm leading-relaxed text-[var(--szu-muted)]">
          同一账号的设备会自动组网，通过 Loro CRDT 自动合并登录态、题库与已配置的账号密码。 不同 OJ
          站点、不同题库行独立合并；同一项并发修改以最后一次写入为准，不再冻结整项。
        </p>
      </header>

      {status?.error && (
        <Banner status="danger">
          <Banner.Title>设备同步网络未运行</Banner.Title>
          <Banner.Description>{status.error}</Banner.Description>
        </Banner>
      )}

      <Card variant="outlined">
        <div slot="headline" className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div
              className="flex size-10 items-center justify-center rounded-xl bg-[var(--szu-brand-soft)] text-[var(--szu-brand)]"
              aria-hidden="true"
            >
              <LuRefreshCw className="size-5" />
            </div>
            <span className="font-semibold">设备同步</span>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="tonal"
              disabled={!status?.enabled || !loaded || syncing}
              onClick={() => void runSync()}
            >
              {syncing ? <LoadingIndicator size={16} /> : <LuRefreshCw className="size-4" />}
              立即同步
            </Button>
            <Switch
              checked={Boolean(status?.enabled)}
              disabled={!loaded || toggling}
              onChange={(event: any) => {
                const next = Boolean(event.detail?.checked ?? event.target?.checked);
                void toggleEnabled(next);
              }}
              aria-label="启用设备同步"
            />
          </div>
        </div>
        <div slot="subhead">
          开启后才会启动组网节点。关闭时不会监听任何端口，也不会处理对端请求。
        </div>

        <dl className="mt-4 grid gap-2 text-sm">
          <div className="flex items-center justify-between gap-4">
            <dt className="text-[var(--szu-muted)]">网络状态</dt>
            <dd className="flex items-center gap-2">
              {!status?.enabled ? (
                <Chip color="default">未启用</Chip>
              ) : status?.running ? (
                <Chip color="success">已连接</Chip>
              ) : (
                <Chip color="warning">启动中</Chip>
              )}
              {toggling && <LoadingIndicator size={16} aria-label="正在切换设备同步" />}
            </dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-[var(--szu-muted)]">本机虚拟 IP</dt>
            <dd className="font-mono text-xs">{status?.overlayIp ?? "—"}</dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-[var(--szu-muted)]">已发现设备</dt>
            <dd>{peerCount === 0 ? "暂无其它设备" : `${onlineCount} / ${peerCount} 台在线`}</dd>
          </div>
        </dl>
      </Card>

      <Card variant="outlined">
        <div slot="headline">本机名称</div>
        <div slot="subhead">只用于设备列表展示，不影响网络身份。</div>
        <div className="mt-4 flex flex-wrap items-center gap-3">
          <TextField
            className="min-w-48 flex-1"
            label="设备名称"
            value={nameDraft}
            disabled={!loaded || savingName}
            {...bindField(setNameDraft)}
          />
          <Button variant="tonal" disabled={!loaded || savingName} onClick={() => void saveName()}>
            {savingName && <LoadingIndicator size={16} />}
            保存
          </Button>
        </div>
      </Card>

      <Card variant="outlined">
        <div slot="headline">网络内的设备</div>
        <div slot="subhead">列表只显示同一账号的设备；点击刷新可立即探测。</div>
        {devices.length === 0 ? (
          <p className="mt-4 text-sm text-[var(--szu-muted)]">
            {status?.enabled ? "正在等待其它设备加入…" : "开启设备同步后，这里会显示同账号的设备。"}
          </p>
        ) : (
          <List aria-label="同步网络内的设备" className="mt-2">
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

      <Card variant="outlined">
        <div slot="headline">同步内容</div>
        <div slot="subhead">
          只同步下列白名单。账号资料、统一认证会话、畅课/巡课登录态与本地外观、通知设置不会同步。
        </div>
        <List aria-label="同步项状态" className="mt-2">
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
                      ? "逐题合并"
                      : "整项合并"
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
          <div className="mt-3 grid gap-2">
            {disabledItems.map((item) => (
              <div
                key={item.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-separator px-4 py-3"
              >
                <span className="text-sm">{item.label} 已停止同步</span>
                <Button
                  variant="tonal"
                  disabled={enablingItem !== null}
                  onClick={() => void enableItem(item.id)}
                >
                  {enablingItem === item.id && <LoadingIndicator size={16} />}
                  恢复同步
                </Button>
              </div>
            ))}
          </div>
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

      {status?.enabled && (
        <p className="flex items-center gap-2 text-xs text-[var(--szu-muted)]">
          <LuCheck className="size-3.5" aria-hidden="true" />
          同步开关与设备名只保存在本机；账号资料、统一认证会话与本地外观/通知设置不会同步。
          <LuWifi className="size-3.5" aria-hidden="true" />
        </p>
      )}
    </section>
  );
}
