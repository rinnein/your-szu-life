import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Button, Dialog, LoadingIndicator, Select, SelectOption, TextField } from "material/react";
import { getElectricityRoomFloor, type ElectricitySettings } from "./electricity";
import { bindField } from "./ui";

export type ElectricityBuilding = { id: string; name: string };
export type ElectricityArea = {
  id: string;
  name: string;
  campusId: string;
  provider: string;
  buildings: ElectricityBuilding[];
};
export type ElectricityCampus = { id: string; name: string; areas: ElectricityArea[] };

export type ElectricityFloorRange = { from: number; to: number };

/** 楼栋名称末尾的数字区间表示该楼栋的楼层范围，取最后一段数字区间。 */
const FLOOR_RANGE_PATTERN = /(\d{1,2})\s*[-–—~～至]\s*(\d{1,2})/g;

export function getElectricityBuildingFloorRange(name: string): ElectricityFloorRange | null {
  const matches = Array.from(name.matchAll(FLOOR_RANGE_PATTERN));
  const match = matches[matches.length - 1];
  if (!match) return null;
  const from = Number(match[1]);
  const to = Number(match[2]);
  if (!Number.isInteger(from) || !Number.isInteger(to) || from < 1 || to < from) return null;
  return { from, to };
}

export function filterElectricityBuildings(
  buildings: ElectricityBuilding[],
  floor: number | null,
): ElectricityBuilding[] {
  if (floor === null) return buildings;
  return buildings.filter((building) => {
    const range = getElectricityBuildingFloorRange(building.name);
    // 提取不到楼层范围的楼栋无法判断，保留而不是误删。
    return range === null || (floor >= range.from && floor <= range.to);
  });
}

export function filterElectricityCampuses(
  campuses: ElectricityCampus[],
  floor: number | null,
): ElectricityCampus[] {
  if (floor === null) return campuses;
  const next: ElectricityCampus[] = [];
  for (const campus of campuses) {
    const areas: ElectricityArea[] = [];
    for (const area of campus.areas) {
      const buildings = filterElectricityBuildings(area.buildings, floor);
      // 有楼栋但全部被过滤，说明该区域没有这一层，隐藏区域。
      if (area.buildings.length > 0 && buildings.length === 0) continue;
      areas.push({ ...area, buildings });
    }
    if (campus.areas.length > 0 && areas.length === 0) continue;
    next.push({ ...campus, areas });
  }
  return next;
}

/** 楼层变化后清掉不再可选的上级选择，避免 Select 里留下已被过滤的值。 */
function normalizeSelection(
  value: ElectricitySettings,
  campuses: ElectricityCampus[],
  floor: number | null,
): ElectricitySettings {
  // 选项尚未加载时不要因为空列表把已保存的配置清掉。
  if (campuses.length === 0) return value;
  const filtered = filterElectricityCampuses(campuses, floor);
  const campus = filtered.find((item) => item.id === value.campusId);
  if (!campus) {
    return value.campusId
      ? { ...value, campusId: "", areaId: "", buildingId: "", buildingName: "" }
      : value;
  }
  const area = campus.areas.find((item) => item.id === value.areaId);
  if (!area) {
    return value.areaId ? { ...value, areaId: "", buildingId: "", buildingName: "" } : value;
  }
  const building = area.buildings.find((item) => item.id === value.buildingId);
  if (!building) {
    return value.buildingId ? { ...value, buildingId: "", buildingName: "" } : value;
  }
  return building.name === value.buildingName ? value : { ...value, buildingName: building.name };
}

/** 校验当前选择是否仍落在楼层过滤后的可选项里。 */
export function isElectricitySelectionAvailable(
  campuses: ElectricityCampus[],
  value: ElectricitySettings,
): boolean {
  const floor = getElectricityRoomFloor(value.roomName);
  if (floor === null || !value.campusId || !value.areaId || !value.buildingId) return false;
  // 选项还没加载时无法判断，不要因此阻止用户使用已保存的配置。
  if (campuses.length === 0) return true;
  const campus = filterElectricityCampuses(campuses, floor).find(
    (item) => item.id === value.campusId,
  );
  const area = campus?.areas.find((item) => item.id === value.areaId);
  return Boolean(area?.buildings.some((building) => building.id === value.buildingId));
}

export function useElectricityOptions(enabled = true) {
  const [campuses, setCampuses] = useState<ElectricityCampus[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const request = useRef(0);

  const reload = useCallback(async () => {
    const id = ++request.current;
    setLoading(true);
    setError(undefined);
    try {
      const next = await invoke<ElectricityCampus[]>("get_electricity_options");
      if (id === request.current) setCampuses(Array.isArray(next) ? next : []);
    } catch (cause) {
      if (id === request.current) {
        setError(typeof cause === "string" ? cause : "无法加载宿舍选项，请连接校园网后重试。");
      }
    } finally {
      if (id === request.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (enabled) void reload();
  }, [enabled, reload]);

  return { campuses, loading, error, reload };
}

export function LocationSelect({
  label,
  value,
  options,
  disabled,
  onChange,
}: {
  label: string;
  value: string;
  options: { id: string; name: string }[];
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <Select
      label={label}
      value={value || ""}
      disabled={disabled}
      onChange={(e: any) => onChange(e.detail?.value ?? e.target?.value ?? "")}
    >
      {options.map((option) => (
        <SelectOption key={option.id} value={option.id}>
          {option.name}
        </SelectOption>
      ))}
    </Select>
  );
}

export function ElectricityLocationFields({
  campuses,
  value,
  disabled,
  onChange,
}: {
  campuses: ElectricityCampus[];
  value: ElectricitySettings;
  disabled?: boolean;
  onChange: (next: ElectricitySettings) => void;
}) {
  const roomFloor = getElectricityRoomFloor(value.roomName);
  const filteredCampuses = filterElectricityCampuses(campuses, roomFloor);
  const campus = filteredCampuses.find((item) => item.id === value.campusId);
  const areas = campus?.areas ?? [];
  const area = areas.find((item) => item.id === value.areaId);
  const buildings = area?.buildings ?? [];
  const roomReady = roomFloor !== null;

  const update = (next: ElectricitySettings) =>
    onChange(normalizeSelection(next, campuses, getElectricityRoomFloor(next.roomName)));

  const chooseCampus = (campusId: string) => {
    const nextCampus = filteredCampuses.find((item) => item.id === campusId);
    // 下级只有唯一选项时直接选中，减少一次点击。
    const onlyArea = nextCampus?.areas.length === 1 ? nextCampus.areas[0] : undefined;
    const onlyBuilding = onlyArea?.buildings.length === 1 ? onlyArea.buildings[0] : undefined;
    update({
      ...value,
      campusId,
      areaId: onlyArea?.id ?? "",
      buildingId: onlyBuilding?.id ?? "",
      buildingName: onlyBuilding?.name ?? "",
    });
  };

  const chooseArea = (areaId: string) => {
    const nextArea = areas.find((item) => item.id === areaId);
    const onlyBuilding = nextArea?.buildings.length === 1 ? nextArea.buildings[0] : undefined;
    update({
      ...value,
      areaId,
      buildingId: onlyBuilding?.id ?? "",
      buildingName: onlyBuilding?.name ?? "",
    });
  };

  const chooseBuilding = (buildingId: string) => {
    const nextBuilding = buildings.find((item) => item.id === buildingId);
    update({ ...value, buildingId, buildingName: nextBuilding?.name ?? "" });
  };

  // 键入只更新文本，失焦后才清掉已不可选的位置——避免中间态（如 120 被识别成
  // 1 层）把已选楼栋丢掉；这两个回调的差异由 bindField 的 input/change 分别承接。
  const roomNameField = bindField({
    input: (roomName) => onChange({ ...value, roomName }),
    change: (roomName) => update({ ...value, roomName }),
  });

  return (
    <div className="electricity-fields">
      <TextField
        className="electricity-room-name"
        label="宿舍号"
        name="roomName"
        required
        error={Boolean(value.roomName.trim()) && roomFloor === null}
        supportingText={
          value.roomName.trim() && roomFloor === null
            ? "房间号应为一至两位楼层加两位房间号。"
            : "填写宿舍号后会自动识别楼层，并筛选可用的校区、宿舍区域与楼栋。"
        }
        disabled={disabled}
        value={value.roomName}
        inputMode="numeric"
        {...roomNameField}
        placeholder="如 601 或 1201"
      />
      <TextField
        label="所在楼层"
        name="floorRange"
        value={roomFloor === null ? "" : `${roomFloor} 层`}
        readonly
        placeholder="输入宿舍号后自动识别"
      />
      <LocationSelect
        label="校区"
        value={value.campusId}
        options={filteredCampuses}
        disabled={disabled || !roomReady}
        onChange={chooseCampus}
      />
      <LocationSelect
        label="宿舍区域"
        value={value.areaId}
        options={areas}
        disabled={disabled || !roomReady || !campus}
        onChange={chooseArea}
      />
      <LocationSelect
        label="楼栋"
        value={value.buildingId}
        options={buildings}
        disabled={disabled || !roomReady || !area}
        onChange={chooseBuilding}
      />
      {roomReady && campuses.length > 0 && filteredCampuses.length === 0 && (
        <p className="text-sm text-muted" role="status">
          没有找到包含 {roomFloor} 层的楼栋，请检查宿舍号是否正确。
        </p>
      )}
    </div>
  );
}

export function ElectricityLocationConfirmDialog({
  open,
  campuses,
  value,
  busy,
  confirmLabel = "确认保存",
  onClose,
  onConfirm,
}: {
  open: boolean;
  campuses: ElectricityCampus[];
  value: ElectricitySettings;
  busy?: boolean;
  confirmLabel?: string;
  onClose: () => void;
  onConfirm: () => void;
}) {
  if (!open || typeof document === "undefined") return null;
  const campus = campuses.find((item) => item.id === value.campusId);
  const area = campus?.areas.find((item) => item.id === value.areaId);
  const building = area?.buildings.find((item) => item.id === value.buildingId);
  const floor = getElectricityRoomFloor(value.roomName);
  const range = building ? getElectricityBuildingFloorRange(building.name) : null;

  return createPortal(
    <Dialog
      open
      headline="确认宿舍信息"
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <p className="text-sm text-muted">
        请确认以下宿舍范围是否正确，保存后将用于电费查询与低电量提醒。
      </p>
      <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
        <dt className="text-muted">校区</dt>
        <dd>{campus?.name ?? "—"}</dd>
        <dt className="text-muted">宿舍区域</dt>
        <dd>{area?.name ?? "—"}</dd>
        <dt className="text-muted">楼栋</dt>
        <dd>{building?.name ?? (value.buildingName || "—")}</dd>
        <dt className="text-muted">宿舍号</dt>
        <dd>{value.roomName.trim() || "—"}</dd>
        <dt className="text-muted">所在楼层</dt>
        <dd>{floor === null ? "—" : `${floor} 层`}</dd>
        {range && (
          <>
            <dt className="text-muted">楼栋楼层</dt>
            <dd>
              {range.from}–{range.to} 层
            </dd>
          </>
        )}
      </dl>
      <div slot="actions">
        <Button variant="text" disabled={busy} onClick={onClose}>
          返回修改
        </Button>
        <Button variant="filled" disabled={busy} onClick={onConfirm}>
          {busy && <LoadingIndicator size={16} />}
          {confirmLabel}
        </Button>
      </div>
    </Dialog>,
    document.body,
  );
}
