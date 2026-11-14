/**
 * 雷达签到定位表单：内置地点下拉 + 浏览器定位 + 恢复上次成功定位。
 *
 * 选择语义（与后端 signin_submit_lms_radar / signin_submit_szu 对齐）：
 * - 不改动下拉：走后端默认点解析链（上次成功 → 课表 → 兜底致理楼）；
 * - 选内置楼栋：传 building_id（GCJ-02，SZU 偏移按围栏半径收敛）；
 * - 浏览器定位：传 WGS-84 读数（custom_is_wgs84 = true）；
 * - 恢复上次成功定位：传已转换的 GCJ-02 点（custom_is_wgs84 = false）。
 */
import { useMemo, useState } from "react";
import { Button, Select, SelectOption } from "material/react";
import { toast } from "../ui";
import type { Building, ResolvedPoint } from "./types";
import { isProvisional } from "./types";

export type PointSelection =
  | { kind: "default" }
  | { kind: "building"; buildingId: number; label: string }
  | {
      kind: "custom";
      latitude: number;
      longitude: number;
      isWgs84: boolean;
      label: string;
    };

interface RadarFormProps {
  buildings: Building[];
  /** 默认点解析结果（上次成功 → 课表 → 兜底），只用于展示。 */
  initial?: ResolvedPoint | null;
  disabled?: boolean;
  onSubmit: (selection: PointSelection) => Promise<void>;
  submitLabel?: string;
}

function pointLabel(selection: PointSelection, initial?: ResolvedPoint | null): string {
  switch (selection.kind) {
    case "default":
      return initial
        ? `${initial.label}（默认 · ${
            initial.source === "last_success"
              ? "上次成功定位"
              : initial.source === "timetable"
                ? "课表位置"
                : "兜底地点"
          }）`
        : "默认（上次成功定位 → 课表 → 致理楼）";
    case "building":
      return selection.label;
    case "custom":
      return `${selection.label} (${selection.latitude.toFixed(5)}, ${selection.longitude.toFixed(5)})`;
  }
}

export function RadarForm({
  buildings,
  initial,
  disabled,
  onSubmit,
  submitLabel = "确认提交",
}: RadarFormProps) {
  const [selected, setSelected] = useState("default");
  const [custom, setCustom] = useState<PointSelection | null>(null);
  const [locating, setLocating] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const selection: PointSelection = useMemo(() => {
    if (custom) return custom;
    if (selected.startsWith("b")) {
      const buildingId = Number(selected.slice(1));
      const building = buildings.find((item) => item.id === buildingId);
      return {
        kind: "building",
        buildingId,
        label: building?.name ?? "所选地点",
      };
    }
    return { kind: "default" };
  }, [custom, selected, buildings]);

  const handleGeolocate = () => {
    if (!("geolocation" in navigator)) {
      toast.danger("当前环境不支持浏览器定位");
      return;
    }
    setLocating(true);
    navigator.geolocation.getCurrentPosition(
      (position) => {
        setCustom({
          kind: "custom",
          latitude: position.coords.latitude,
          longitude: position.coords.longitude,
          isWgs84: true,
          label: "浏览器定位",
        });
        setLocating(false);
        toast.success("已获取浏览器定位，将作为自定义坐标提交");
      },
      (error) => {
        setLocating(false);
        toast.danger("定位失败", { description: error.message });
      },
      { enableHighAccuracy: true, timeout: 5000, maximumAge: 3000 },
    );
  };

  const restoreLast = () => {
    if (!initial) {
      toast.warning("暂无上一次成功的定位记录");
      return;
    }
    // 上次成功点存的是提交用的 GCJ-02 坐标，回传时不能再转换。
    setCustom({
      kind: "custom",
      latitude: initial.latitude,
      longitude: initial.longitude,
      isWgs84: false,
      label: initial.label,
    });
    setSelected("default");
  };

  const handleSubmit = async () => {
    setSubmitting(true);
    try {
      await onSubmit(selection);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="flex flex-col gap-3" data-testid="radar-form">
      <Select
        label="签到地点"
        aria-label="选择签到地点"
        className="w-full"
        value={custom ? "default" : selected}
        disabled={disabled || locating}
        onChange={(e: unknown) => {
          const next = (e as { detail?: { value?: unknown } }).detail?.value;
          const target = (e as { target?: { value?: unknown } }).target?.value;
          const picked =
            typeof next === "string" ? next : typeof target === "string" ? target : "default";
          setSelected(picked);
          setCustom(null);
        }}
      >
        <SelectOption value="default">{pointLabel({ kind: "default" }, initial)}</SelectOption>
        {buildings.map((building) => (
          <SelectOption key={building.id} value={`b${building.id}`}>
            {isProvisional(building) ? `${building.name}（坐标待校准）` : building.name}
          </SelectOption>
        ))}
      </Select>

      <p className="text-xs text-muted px-1">
        将提交：<span data-testid="radar-point-label">{pointLabel(selection, initial)}</span>
      </p>

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="tonal" disabled={disabled || locating} onClick={handleGeolocate}>
          {locating ? "定位中…" : "定位"}
        </Button>
        <Button variant="tonal" disabled={disabled} onClick={restoreLast}>
          使用上一次成功的定位
        </Button>
        <Button
          variant="filled"
          disabled={disabled || submitting || locating}
          onClick={() => void handleSubmit()}
        >
          {submitting ? "提交中…" : submitLabel}
        </Button>
      </div>
    </div>
  );
}
