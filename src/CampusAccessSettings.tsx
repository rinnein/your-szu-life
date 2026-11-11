import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Card, List, ListItem, LoadingIndicator, Switch } from "material/react";
import { Banner, Chip, toast } from "./ui";
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

function errorMessage(error: unknown, fallback: string) {
  return typeof error === "string" ? error : error instanceof Error ? error.message : fallback;
}

function campusLabel(device: CampusMeshDevice) {
  if (device.isSelf) {
    return device.onCampus ? "校内" : "校外";
  }
  if (!device.reachable) return "离线";
  if (!device.campusAccess) return "未开启共享";
  return device.onCampus ? "校内" : "校外";
}

export function CampusAccessSettings() {
  const [status, setStatus] = useState<CampusMeshStatus | null>(null);
  const [devices, setDevices] = useState<CampusMeshDevice[]>([]);
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const apply = useCallback((next: CampusMeshStatus) => setStatus(next), []);

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const subscribe = async () => {
      unlisten.push(
        await listen<CampusMeshStatus>("mesh-status-changed", ({ payload }) => {
          if (!disposed) apply(payload);
        }),
      );
      unlisten.push(
        await listen<CampusMeshDevice[]>("mesh-devices-changed", ({ payload }) => {
          if (!disposed) setDevices(payload);
        }),
      );
      try {
        const [nextStatus, nextDevices] = await Promise.all([
          invoke<CampusMeshStatus>("get_mesh_status"),
          invoke<CampusMeshDevice[]>("list_mesh_devices"),
        ]);
        if (disposed) return;
        apply(nextStatus);
        setDevices(nextDevices);
      } catch (error) {
        if (!disposed)
          toast.danger("读取校园网访问状态失败", { description: errorMessage(error, "") });
      } finally {
        if (!disposed) setLoaded(true);
      }
    };
    void subscribe();
    return () => {
      disposed = true;
      for (const stop of unlisten) stop();
    };
  }, [apply]);

  const toggle = async (enabled: boolean) => {
    setBusy(true);
    try {
      const next = await invoke<CampusMeshStatus>("set_mesh_campus_access", { enabled });
      apply(next);
      toast.success(enabled ? "已开启校园网访问" : "已关闭校园网访问", {
        description: enabled
          ? "在校设备会把校园网出口共享给本账号的其它设备。"
          : "已恢复本机原有的出网逻辑。",
      });
    } catch (error) {
      toast.danger("切换校园网访问失败", { description: errorMessage(error, "") });
    } finally {
      setBusy(false);
    }
  };

  const self = devices.find((device) => device.isSelf);
  const localOnCampus = Boolean(self?.onCampus);
  const campusDevices = devices.filter(
    (device) => !device.isSelf && device.reachable && device.campusAccess && device.onCampus,
  );
  const enabled = Boolean(status?.campusAccess);

  return (
    <Card variant="outlined">
      <div slot="headline" className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div
            className="flex size-10 items-center justify-center rounded-xl bg-[var(--szu-brand-soft)] text-[var(--szu-brand)]"
            aria-hidden="true"
          >
            <LuWifi className="size-5" />
          </div>
          <span className="font-semibold">校园网访问</span>
        </div>
        <Switch
          checked={enabled}
          disabled={!loaded || busy}
          onChange={(event: any) => {
            const next = Boolean(event.detail?.checked ?? event.target?.checked);
            void toggle(next);
          }}
          aria-label="启用校园网访问"
        />
      </div>
      <div slot="subhead">
        开启后，同一账号的在校设备会共享校园网出口。只影响本应用的校内请求，不会代理其它软件，也不需要
        VPN。
      </div>

      <dl className="mt-4 grid gap-2 text-sm">
        <div className="flex items-center justify-between gap-4">
          <dt className="text-[var(--szu-muted)]">本机网络</dt>
          <dd>
            {localOnCampus ? <Chip color="success">校内</Chip> : <Chip color="default">校外</Chip>}
          </dd>
        </div>
        <div className="flex items-center justify-between gap-4">
          <dt className="text-[var(--szu-muted)]">当前出口</dt>
          <dd className="flex items-center gap-2">
            {status?.campusEgress ? (
              <>
                <LuCheck className="size-4 text-[var(--md-sys-color-primary)]" aria-hidden="true" />
                {status.campusEgress}
              </>
            ) : (
              <span className="text-[var(--szu-muted)]">
                {localOnCampus ? "本机在校内，直接访问" : "未使用"}
              </span>
            )}
            {busy && <LoadingIndicator size={16} aria-label="正在切换校园网访问" />}
          </dd>
        </div>
      </dl>

      {enabled && !localOnCampus && campusDevices.length === 0 && (
        <Banner status="warning" className="mt-4">
          <Banner.Title>没有可用的校园网出口</Banner.Title>
          <Banner.Description>
            需要另一台已登录同一账号、开启校园网访问且当前处于校园网的设备。发现设备后会自动使用延迟最低的一台。
          </Banner.Description>
        </Banner>
      )}

      <p className="mt-4 text-xs text-[var(--szu-muted)]">账号内的设备</p>
      {devices.length === 0 ? (
        <p className="mt-1 text-sm text-[var(--szu-muted)]">
          {enabled ? "正在等待其它设备加入…" : "开启后这里会列出同账号设备及其校园网状态。"}
        </p>
      ) : (
        <List aria-label="校园网访问设备列表" className="mt-1">
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
        <p className="mt-3 text-xs text-[var(--szu-muted)]">
          可用出口：{campusDevices.map((device) => device.name).join("、")}
        </p>
      )}
    </Card>
  );
}
