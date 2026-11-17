/**
 * 签到地点选择：默认点（后端解析链）+ 内置楼栋 + 浏览器定位。
 *
 * 受控组件：选择状态由对话框持有（单一 PointSelection，不再用
 * 「下拉值 + 自定义点」两份状态拼装）；定位过程按 usePending("locate")
 * 登记，卸载后的定位回调不再写状态。
 */
import { useEffect, useRef } from "react";
import { Button, LoadingIndicator, Select, SelectOption } from "material/react";
import { LuLocateFixed } from "react-icons/lu";
import { toast } from "../ui";
import type { Async } from "../async-state";
import { usePending } from "../use-pending";
import type { Building, ResolvedPoint } from "./types";
import { isProvisional } from "./types";
import { defaultPointLabel, type PointSelection } from "./point-selection";

interface PointPickerProps {
  buildings: Building[];
  /** 楼栋列表读取失败时提示「内置地点不可用」，而不是静默只剩默认项。 */
  buildingsFailed?: boolean;
  /** 默认点解析结果（上次成功 → 课表 → 兜底），只用于展示；非雷达场景为 null。 */
  resolved: Async<ResolvedPoint | null>;
  value: PointSelection;
  onChange: (selection: PointSelection) => void;
}

function readSelectValue(event: unknown): string {
  const detail = (event as { detail?: { value?: unknown } }).detail?.value;
  const target = (event as { target?: { value?: unknown } }).target?.value;
  return typeof detail === "string" ? detail : typeof target === "string" ? target : "default";
}

function selectionLabel(selection: PointSelection, resolved: Async<ResolvedPoint | null>): string {
  switch (selection.kind) {
    case "default":
      return defaultPointLabel(resolved.status === "ready" ? resolved.data : null, resolved.status);
    case "building":
      return selection.label;
    case "custom":
      return `${selection.label} (${selection.latitude.toFixed(5)}, ${selection.longitude.toFixed(5)})`;
  }
}

export function PointPicker({
  buildings,
  buildingsFailed,
  resolved,
  value,
  onChange,
}: PointPickerProps) {
  const pending = usePending();
  const locating = pending.isPending("locate");
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const locate = () => {
    if (!("geolocation" in navigator)) {
      toast.danger("当前环境不支持浏览器定位");
      return;
    }
    void pending.run(
      "locate",
      () =>
        new Promise<void>((resolve, reject) => {
          navigator.geolocation.getCurrentPosition(
            (position) => {
              // 对话框可能已经关闭：仍要 resolve 清理 pending，但不再写状态。
              if (live.current) {
                onChange({
                  kind: "custom",
                  latitude: position.coords.latitude,
                  longitude: position.coords.longitude,
                  isWgs84: true,
                  label: "当前位置",
                });
              }
              resolve();
            },
            (error) => reject(new Error(error.message)),
            { enableHighAccuracy: true, timeout: 5000, maximumAge: 3000 },
          );
        }),
    );
  };

  return (
    <div className="flex flex-col gap-3" data-testid="point-picker">
      <Select
        label="签到地点"
        aria-label="选择签到地点"
        className="w-full"
        value={
          value.kind === "custom"
            ? "custom"
            : value.kind === "building"
              ? `b${value.buildingId}`
              : "default"
        }
        disabled={locating}
        onChange={(event: unknown) => {
          const picked = readSelectValue(event);
          if (picked === "custom") return; // 自定义项只在已定位时出现，重复选择无变化
          if (picked.startsWith("b")) {
            const buildingId = Number(picked.slice(1));
            const building = buildings.find((item) => item.id === buildingId);
            if (building) {
              onChange({ kind: "building", buildingId, label: building.name });
            }
            return;
          }
          onChange({ kind: "default" });
        }}
      >
        <SelectOption value="default">{selectionLabel({ kind: "default" }, resolved)}</SelectOption>
        {buildings.map((building) => (
          <SelectOption key={building.id} value={`b${building.id}`}>
            {isProvisional(building) ? `${building.name}（坐标待校准）` : building.name}
          </SelectOption>
        ))}
        {value.kind === "custom" && (
          <SelectOption value="custom">{selectionLabel(value, resolved)}</SelectOption>
        )}
      </Select>

      <p className="status-line" data-testid="point-label">
        将提交：{selectionLabel(value, resolved)}
        {resolved.status === "error" && "；默认地点解析失败，将按后端默认链提交"}
        {buildingsFailed && "；内置地点列表不可用，可选择浏览器定位"}
      </p>

      <div>
        <Button variant="tonal" disabled={locating} onClick={locate}>
          {locating ? <LoadingIndicator size={16} /> : <LuLocateFixed aria-hidden="true" />}
          使用当前位置
        </Button>
      </div>
    </div>
  );
}
