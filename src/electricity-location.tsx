import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Select, SelectOption, TextField } from "material/react";
import { getElectricityRoomFloor, type ElectricitySettings } from "./electricity";

export type ElectricityBuilding = { id: string; name: string };
export type ElectricityArea = {
  id: string;
  name: string;
  campusId: string;
  provider: string;
  buildings: ElectricityBuilding[];
};
export type ElectricityCampus = { id: string; name: string; areas: ElectricityArea[] };

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
  const campus = campuses.find((item) => item.id === value.campusId);
  const area = campus?.areas.find((item) => item.id === value.areaId);
  const buildings = area?.buildings ?? [];
  const roomFloor = getElectricityRoomFloor(value.roomName);

  const chooseCampus = (campusId: string) => {
    const nextCampus = campuses.find((item) => item.id === campusId);
    const nextArea = nextCampus?.areas[0];
    const nextBuilding = nextArea?.buildings[0];
    onChange({
      ...value,
      campusId,
      areaId: nextArea?.id ?? "",
      buildingId: nextBuilding?.id ?? "",
      buildingName: nextBuilding?.name ?? "",
    });
  };

  const chooseArea = (areaId: string) => {
    const nextArea = campus?.areas.find((item) => item.id === areaId);
    const nextBuilding = nextArea?.buildings[0];
    onChange({
      ...value,
      areaId,
      buildingId: nextBuilding?.id ?? "",
      buildingName: nextBuilding?.name ?? "",
    });
  };

  const chooseBuilding = (buildingId: string) => {
    const nextBuilding = buildings.find((item) => item.id === buildingId);
    onChange({ ...value, buildingId, buildingName: nextBuilding?.name ?? "" });
  };

  return (
    <div className="electricity-fields">
      <LocationSelect
        label="校区"
        value={value.campusId}
        options={campuses}
        disabled={disabled}
        onChange={chooseCampus}
      />
      <LocationSelect
        label="宿舍区域"
        value={value.areaId}
        options={campus?.areas ?? []}
        disabled={disabled || !campus}
        onChange={chooseArea}
      />
      <LocationSelect
        label="楼栋"
        value={value.buildingId}
        options={buildings}
        disabled={disabled || !area}
        onChange={chooseBuilding}
      />
      <TextField
        label="宿舍号"
        name="roomName"
        required
        error={Boolean(value.roomName.trim()) && roomFloor === null}
        supportingText="房间号应为一至两位楼层加两位房间号。"
        disabled={disabled}
        value={value.roomName}
        inputMode="numeric"
        onChange={(e: any) =>
          onChange({ ...value, roomName: e.detail?.value ?? e.target?.value ?? "" })
        }
        placeholder="如 601 或 1201"
      />
      {area && (
        <TextField
          label="楼层范围"
          name="floorRange"
          value={roomFloor === null ? "" : `${roomFloor} 层`}
          readonly
          placeholder="输入宿舍号后自动识别"
        />
      )}
    </div>
  );
}
