import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, LoadingIndicator } from "material/react";
import { Banner, toast } from "../ui";
import type { NotificationConfig } from "../NotificationSettings";
import {
  getElectricityRoomFloor,
  hasElectricityData,
  type ElectricityResult,
  type ElectricitySettings,
} from "../electricity";
import {
  ElectricityLocationConfirmDialog,
  ElectricityLocationFields,
  isElectricitySelectionAvailable,
  useElectricityOptions,
} from "../electricity-location";
import type { StepProps } from "./step-types";

const EMPTY: ElectricitySettings = {
  campusId: "",
  areaId: "",
  buildingId: "",
  buildingName: "",
  roomName: "",
};

export function DormStep({ onComplete }: StepProps) {
  const {
    campuses,
    loading: optionsLoading,
    error: optionsError,
    reload,
  } = useElectricityOptions(true);
  const [settings, setSettings] = useState<ElectricitySettings>(EMPTY);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  useEffect(() => {
    let active = true;
    void invoke<ElectricitySettings | null>("get_electricity_settings")
      .then((saved) => {
        if (active && saved) {
          setSettings({ ...saved, roomName: saved.roomName.trim() });
        }
      })
      .catch((cause) => {
        if (active) {
          toast.danger("无法读取已保存的宿舍信息", {
            description: typeof cause === "string" ? cause : "可以重新填写宿舍信息。",
          });
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const roomFloor = getElectricityRoomFloor(settings.roomName);
  const canSave = Boolean(
    settings.campusId &&
    settings.areaId &&
    settings.buildingId &&
    settings.roomName.trim() &&
    roomFloor !== null &&
    isElectricitySelectionAvailable(campuses, settings),
  );

  const save = async () => {
    if (!canSave) {
      toast.warning(
        settings.roomName.trim() && roomFloor === null
          ? "宿舍号需为一至两位楼层加两位房间号，例如 601 或 1201。"
          : "请先选择校区、宿舍区域、楼栋并填写宿舍号。",
      );
      setConfirmOpen(false);
      return;
    }
    setBusy(true);
    let saved = false;
    try {
      const normalized = { ...settings, roomName: settings.roomName.trim() };
      // 先确认能查到电费信息，再落库；查询失败或没有读数时保持未保存状态。
      const result = await invoke<ElectricityResult>("query_electricity", {
        input: normalized,
      });
      if (!hasElectricityData(result)) {
        toast.warning("未查询到该宿舍的电费信息", {
          description: "请检查校区、宿舍区域、楼栋和宿舍号是否正确，配置尚未保存。",
        });
        return;
      }
      await invoke("save_electricity_settings", { settings: normalized });
      saved = true;
      setSettings(normalized);
      // 宿舍位置保存后自动启用低电量提醒；保留其它通知开关与总开关。
      try {
        const notifications = await invoke<NotificationConfig>("get_notification_settings");
        if (!notifications.electricity) {
          await invoke("save_notification_settings", {
            settings: { ...notifications, electricity: true },
          });
        }
      } catch {
        // 位置已保存，提醒开关保持原样，用户仍可在设置中开启。
      }
      toast.success("宿舍信息已保存，电费查询与低电量提醒已启用", {
        description:
          result.currentRemaining == null
            ? undefined
            : `当前剩余 ${result.currentRemaining.toFixed(2)} 度`,
      });
      await onComplete();
    } catch (cause) {
      toast.danger(
        saved ? "宿舍信息已保存，但未能完成后续步骤" : "未能查询到宿舍电费信息，配置尚未保存",
        {
          description: typeof cause === "string" ? cause : "请检查宿舍信息和校园网连接后重试。",
        },
      );
    } finally {
      setBusy(false);
      setConfirmOpen(false);
    }
  };

  return (
    <Card variant="outlined">
      <div slot="headline">宿舍信息</div>
      <div slot="subhead">
        填写后会自动启用宿舍电量查询与低电量提醒，不需要在设置里重复配置。没有住宿或暂时不想填写可以跳过。
      </div>
      <div className="p-4 grid gap-3">
        {optionsError && (
          <Banner status="warning">
            <Banner.Title>无法加载宿舍选项</Banner.Title>
            <Banner.Description>
              {optionsError} 可连接校园网后重试，或先跳过此步骤。
            </Banner.Description>
          </Banner>
        )}
        {optionsError && (
          <Button variant="outlined" disabled={optionsLoading} onClick={() => void reload()}>
            重试加载宿舍选项
          </Button>
        )}
        {loading ? (
          <div className="flex items-center gap-2 text-sm text-muted" role="status">
            <LoadingIndicator size={16} /> 正在读取已保存的宿舍信息…
          </div>
        ) : (
          <ElectricityLocationFields
            campuses={campuses}
            value={settings}
            disabled={busy || optionsLoading}
            onChange={setSettings}
          />
        )}
        <Button
          variant="filled"
          disabled={busy || loading || optionsLoading || !canSave}
          onClick={() => setConfirmOpen(true)}
        >
          {busy && <LoadingIndicator size={16} />}
          {busy ? "保存中…" : "保存并继续"}
        </Button>
      </div>
      <ElectricityLocationConfirmDialog
        open={confirmOpen}
        campuses={campuses}
        value={settings}
        busy={busy}
        confirmLabel="确认并保存"
        onClose={() => {
          if (!busy) setConfirmOpen(false);
        }}
        onConfirm={() => void save()}
      />
    </Card>
  );
}
