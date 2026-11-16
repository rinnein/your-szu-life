import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, List, ListItem, LoadingIndicator, Switch } from "material/react";
import { Banner, Chip, SettingList, SettingRow, toast } from "./ui";
import { errorMessage, useAsyncLoad } from "./async-state";
import { usePending } from "./use-pending";
import { LuCheck, LuWifi } from "react-icons/lu";

export type CampusMeshStatus = {
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

export type CampusMeshDevice = {
  deviceId: string;
  name: string;
  platform: string;
  overlayIp: string | null;
  isSelf: boolean;
  reachable: boolean;
  campusAccess: boolean;
  onCampus: boolean;
  region: string;
  networkState: string;
  ssid: string | null;
};

type CampusSnapshot = { status: CampusMeshStatus; devices: CampusMeshDevice[] };

function campusLabel(device: CampusMeshDevice) {
  if (device.isSelf) {
    return device.onCampus ? "校内" : "校外";
  }
  if (!device.reachable) return "离线";
  if (!device.campusAccess) return "未开启共享";
  return device.onCampus ? "校内" : "校外";
}

export function CampusAccessSettings() {
  const pending = usePending();
  /** 事件推送与本机切换都会改写它，因此不直接复用读取结果。 */
  const [snapshot, setSnapshot] = useState<CampusSnapshot | null>(null);

  const { state, reload } = useAsyncLoad(async () => {
    const [status, devices] = await Promise.all([
      invoke<CampusMeshStatus>("get_mesh_status"),
      invoke<CampusMeshDevice[]>("list_mesh_devices"),
    ]);
    return { status, devices };
  });

  useEffect(() => {
    if (state.status === "ready") setSnapshot(state.data);
  }, [state]);

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const subscribe = async () => {
      unlisten.push(
        await listen<CampusMeshStatus>("mesh-status-changed", ({ payload }) => {
          if (!disposed)
            setSnapshot((current) => ({ status: payload, devices: current?.devices ?? [] }));
        }),
      );
      unlisten.push(
        await listen<CampusMeshDevice[]>("mesh-devices-changed", ({ payload }) => {
          if (!disposed) {
            setSnapshot((current) => (current ? { ...current, devices: payload } : current));
          }
        }),
      );
    };
    void subscribe();
    return () => {
      disposed = true;
      for (const stop of unlisten) stop();
    };
  }, []);

  const toggle = async (enabled: boolean) => {
    const result = await pending.run("toggle", () =>
      invoke<CampusMeshStatus>("set_mesh_campus_access", { enabled }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("切换校园网访问失败", { description: errorMessage(result.error, "") });
      return;
    }
    // 开关不做乐观更新：保持服务端值直到命令返回。
    setSnapshot((current) => (current ? { ...current, status: result.value } : current));
    toast.success(enabled ? "已开启校园网访问" : "已关闭校园网访问", {
      description: enabled
        ? "在校设备会把校园网出口共享给本账号的其它设备。"
        : "已恢复本机原有的出网逻辑。",
    });
  };

  const status = snapshot?.status ?? null;
  const devices = snapshot?.devices ?? [];
  const self = devices.find((device) => device.isSelf);
  const localOnCampus = Boolean(self?.onCampus);
  const campusDevices = devices.filter(
    (device) => !device.isSelf && device.reachable && device.campusAccess && device.onCampus,
  );
  const enabled = Boolean(status?.campusAccess);
  const switching = pending.isPending("toggle");
  const ready = state.status === "ready" && snapshot !== null;

  return (
    <Card variant="outlined">
      <div slot="headline">校园网访问</div>
      <div slot="subhead">
        开启后，同一账号的在校设备会共享校园网出口。只影响本应用的校内请求，不会代理其它软件，也不需要
        VPN。
      </div>

      {state.status === "error" && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>读取校园网访问状态失败</Banner.Title>
            <Banner.Description>{state.message}</Banner.Description>
          </Banner>
          <Button variant="outlined" onClick={() => void reload()}>
            重试
          </Button>
        </div>
      )}

      <SettingList>
        <SettingRow
          icon={<LuWifi aria-hidden="true" />}
          title="启用校园网访问"
          description="只影响本应用的校内请求，不需要 VPN。"
        >
          <Switch
            checked={enabled}
            disabled={!ready || switching}
            onChange={(event: any) => {
              const next = Boolean(event.detail?.checked ?? event.target?.checked);
              void toggle(next);
            }}
            aria-label="启用校园网访问"
          />
        </SettingRow>
        <SettingRow title="本机网络">
          {localOnCampus ? <Chip color="success">校内</Chip> : <Chip color="default">校外</Chip>}
        </SettingRow>
        <SettingRow title="当前出口">
          <span className="flex items-center gap-2 text-sm">
            {status?.campusEgress ? (
              <>
                <LuCheck className="size-4 text-primary" aria-hidden="true" />
                {status.campusEgress}
              </>
            ) : (
              <span className="text-muted">
                {localOnCampus ? "本机在校内，直接访问" : "未使用"}
              </span>
            )}
            {switching && <LoadingIndicator size={16} aria-label="正在切换校园网访问" />}
          </span>
        </SettingRow>
      </SettingList>

      {enabled && !localOnCampus && campusDevices.length === 0 && (
        <Banner status="warning" className="mt-4">
          <Banner.Title>没有可用的校园网出口</Banner.Title>
          <Banner.Description>
            需要另一台已登录同一账号、开启校园网访问且当前处于校园网的设备。发现设备后会自动使用延迟最低的一台。
          </Banner.Description>
        </Banner>
      )}

      <div className="card-group mt-4">
        <p className="card-group-title">账号内的设备</p>
        {devices.length === 0 ? (
          <p className="text-sm text-muted">
            {enabled ? "正在等待其它设备加入…" : "开启后这里会列出同账号设备及其校园网状态。"}
          </p>
        ) : (
          <List aria-label="校园网访问设备列表">
            {devices.map((device) => (
              <ListItem
                key={device.deviceId || device.overlayIp || device.name}
                value={device.deviceId || device.overlayIp || device.name}
                headline={device.name}
                supportingText={
                  device.overlayIp
                    ? `${device.overlayIp}${device.ssid ? ` · ${device.ssid}` : ""}`
                    : "等待分配虚拟 IP"
                }
                trailingText={campusLabel(device)}
              />
            ))}
          </List>
        )}
        {enabled && campusDevices.length > 0 && (
          <p className="text-xs text-muted">
            可用出口：{campusDevices.map((device) => device.name).join("、")}
          </p>
        )}
      </div>
    </Card>
  );
}
