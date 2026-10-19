import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Alert,
  Button,
  Card,
  FieldError,
  Form,
  Input,
  Label,
  ListBox,
  Select,
  Spinner,
  TextField,
  toast,
} from "@heroui/react";
import { LuZap } from "react-icons/lu";
import { copyText } from "./clipboard";
import { ElectricityDetails } from "./ElectricityDetails";
import { Card as UiCard, SkeletonCard } from "./components";
import {
  getElectricityConstants,
  getElectricityRoomFloor,
  type ElectricitySettings as Settings,
  type ElectricityResult as Result,
  type ElectricityRecharge,
} from "./electricity";

type Building = { id: string; name: string };
type Area = { id: string; name: string; campusId: string; provider: string; buildings: Building[] };
type Campus = { id: string; name: string; areas: Area[] };
function LocationSelect({
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
      aria-label={label}
      selectedKey={value || null}
      isDisabled={disabled}
      onSelectionChange={(key) => onChange(String(key))}
    >
      <Label>{label}</Label>
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox aria-label={label} items={options}>
          {(option) => (
            <ListBox.Item id={option.id} textValue={option.name}>
              {option.name}
            </ListBox.Item>
          )}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}

export default function CampusPaymentsElectricityPage() {
  const [campuses, setCampuses] = useState<Campus[]>([]);
  const [settings, setSettings] = useState<Settings>({
    campusId: "",
    areaId: "",
    buildingId: "",
    buildingName: "",
    roomName: "",
  });
  const [result, setResult] = useState<Result>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const [loadingOptions, setLoadingOptions] = useState(false);
  const [optionsReady, setOptionsReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState(false);
  const [rate, setRate] = useState<number>();
  const [savedSettings, setSavedSettings] = useState<Settings>();
  const [importText, setImportText] = useState("");
  const [shareMessage, setShareMessage] = useState("");
  const [exportText, setExportText] = useState("");
  const [sharing, setSharing] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const requestId = useRef(0);
  useEffect(() => {
    let active = true;
    void getElectricityConstants()
      .then((constants) => {
        if (active) setRate(constants?.yuanPerKwh);
      })
      .catch(() => {
        if (active) setError("无法加载电价换算比例，请重新打开模块。");
      });
    return () => {
      active = false;
    };
  }, []);
  const initialSettingsRequest = useRef<Promise<Settings | null> | undefined>(undefined);
  const optionsRequest = useRef<Promise<Campus[]> | undefined>(undefined);
  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        initialSettingsRequest.current ??= invoke<Settings | null>("get_electricity_settings");
        const saved = await initialSettingsRequest.current;
        if (!active) return;
        if (!saved) {
          setLoading(false);
          return;
        }
        setSettings(saved);
        setSavedSettings(saved);
        setLoading(false);
        setBusy(true);
        const id = ++requestId.current;
        // Cache reads are independent of portal options and refresh requests.
        // Even offline, a previously visited room can render immediately.
        const cached = await invoke<Result | null>("get_cached_electricity", {
          input: saved,
        }).catch(() => null);
        if (!active || id !== requestId.current) return;
        if (cached) {
          setResult(cached);
          setEditing(false);
        }
        try {
          const next = await invoke<Result>("query_electricity", { input: saved });
          if (active && id === requestId.current) {
            setResult(next);
            setEditing(false);
            setError(next.cache?.refreshError || undefined);
          }
        } catch (e) {
          if (active && id === requestId.current)
            setError(typeof e === "string" ? e : "本次更新未完成，已有记录仍可查看。");
        } finally {
          if (active && id === requestId.current) setBusy(false);
        }
      } catch (e) {
        if (active) {
          setError(typeof e === "string" ? e : "无法读取宿舍配置。");
          setBusy(false);
        }
      } finally {
        if (active) setLoading(false);
      }
    };
    void load();
    return () => {
      active = false;
      requestId.current += 1;
    };
  }, []);
  useEffect(() => {
    if (loading || (savedSettings && !editing)) {
      setLoadingOptions(false);
      return;
    }
    let active = true;
    setLoadingOptions(true);
    optionsRequest.current ??= invoke<Campus[]>("get_electricity_options");
    void optionsRequest.current
      .then((next) => {
        if (active) {
          setCampuses(Array.isArray(next) ? next : []);
          setOptionsReady(true);
        }
      })
      .catch((e) => {
        optionsRequest.current = undefined;
        if (active) setError(typeof e === "string" ? e : "无法加载宿舍选项，请连接校园网后重试。");
      })
      .finally(() => {
        if (active) setLoadingOptions(false);
      });
    return () => {
      active = false;
    };
  }, [loading, savedSettings, editing]);
  const campus = campuses.find((x) => x.id === settings.campusId);
  const area = campus?.areas.find((x) => x.id === settings.areaId);
  const buildings = area?.buildings ?? [];
  const roomFloor = getElectricityRoomFloor(settings.roomName);
  const canQuery = Boolean(
    settings.campusId &&
    settings.areaId &&
    settings.buildingId &&
    settings.roomName.trim() &&
    roomFloor !== null,
  );
  const chooseCampus = (campusId: string) => {
    requestId.current += 1;
    const next = campuses.find((x) => x.id === campusId);
    const nextArea = next?.areas[0];
    const nextBuilding = nextArea?.buildings[0];
    setSettings((s) => ({
      ...s,
      campusId,
      areaId: nextArea?.id ?? "",
      buildingId: nextBuilding?.id ?? "",
      buildingName: nextBuilding?.name ?? "",
    }));
    setResult(undefined);
  };
  const chooseArea = (areaId: string) => {
    requestId.current += 1;
    const next = campus?.areas.find((x) => x.id === areaId);
    const nextBuilding = next?.buildings[0];
    setSettings((s) => ({
      ...s,
      areaId,
      buildingId: nextBuilding?.id ?? "",
      buildingName: nextBuilding?.name ?? "",
    }));
    setResult(undefined);
  };
  const chooseBuilding = (buildingId: string) => {
    requestId.current += 1;
    setResult(undefined);
    const next = buildings.find((x) => x.id === buildingId);
    setSettings((s) => ({ ...s, buildingId, buildingName: next?.name ?? "" }));
  };
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canQuery) {
      const msg =
        settings.roomName.trim() && roomFloor === null
          ? "宿舍号需为一至两位楼层加两位房间号，例如 601 或 1201。"
          : "请先选择校区、宿舍区域、楼栋并填写宿舍号。";
      setError(msg);
      toast.warning(msg);
      return;
    }
    const id = ++requestId.current;
    setBusy(true);
    setError(undefined);
    try {
      await invoke("save_electricity_settings", { settings });
      if (id !== requestId.current) return;
      setSavedSettings(settings);
      const next = await invoke<Result>("query_electricity", {
        input: {
          ...settings,
        },
      });
      if (id === requestId.current) {
        setResult(next);
        setEditing(false);
        setError(next.cache?.refreshError || undefined);
        if (next.cache?.refreshError) {
          toast.warning("电费刷新未完成", { description: next.cache.refreshError });
        }
      }
    } catch (e) {
      if (id === requestId.current) {
        const msg = typeof e === "string" ? e : "电费查询失败，请检查宿舍信息和校园网连接。";
        setError(msg);
        toast.danger("查询未完成", { description: msg });
      }
    } finally {
      if (id === requestId.current) setBusy(false);
    }
  };
  const refreshLatest = async () => {
    if (!savedSettings || busy) return;
    const id = ++requestId.current;
    setBusy(true);
    setError(undefined);
    try {
      const next = await invoke<Result>("query_electricity", {
        input: savedSettings,
        forceRefresh: true,
      });
      if (id === requestId.current) {
        setResult(next);
        setError(next.cache?.refreshError || undefined);
      }
    } catch (e) {
      if (id === requestId.current)
        setError(typeof e === "string" ? e : "更新未完成，已有记录仍可查看。");
    } finally {
      if (id === requestId.current) setBusy(false);
    }
  };
  const exportConfig = async () => {
    setSharing(true);
    setShareMessage("");
    setExportText("");
    try {
      const text = await invoke<string>("export_electricity_settings");
      if (await copyText(text)) setShareMessage("宿舍配置已复制。");
      else {
        setExportText(text);
        setShareMessage("无法自动复制，请选择下方配置码手动复制。");
      }
    } catch (e) {
      setShareMessage(typeof e === "string" ? e : "导出失败，请重试。");
    } finally {
      setSharing(false);
    }
  };
  const importConfig = async () => {
    setSharing(true);
    setShareMessage("");
    try {
      const imported = await invoke<Settings>("import_electricity_settings", { text: importText });
      requestId.current += 1;
      setSettings(imported);
      setEditing(true);
      setResult(undefined);
      setShareMessage("已填入宿舍配置，点击查询电费保存并查询。");
    } catch (e) {
      setShareMessage(typeof e === "string" ? e : "导入失败，请检查配置码。");
    } finally {
      setSharing(false);
    }
  };
  const retryRecharges = async () => {
    if (!savedSettings || retrying) return;
    const id = requestId.current;
    setRetrying(true);
    try {
      const recharges = await invoke<ElectricityRecharge[]>("query_electricity_recharges", {
        input: savedSettings,
      });
      if (id === requestId.current) setResult((r) => r && { ...r, recharges, rechargeError: null });
    } catch (e) {
      if (id === requestId.current)
        setResult(
          (r) => r && { ...r, rechargeError: typeof e === "string" ? e : "充值记录查询失败" },
        );
    } finally {
      setRetrying(false);
    }
  };
  const formDisabled =
    loading ||
    busy ||
    sharing ||
    ((!savedSettings || editing) && (loadingOptions || !optionsReady));
  return (
    <div className="electricity-page">
      <Card className="payments-card">
        <Card.Header>
          <Card.Title className="payments-card-title">
            <LuZap aria-hidden="true" />
            查电量
          </Card.Title>
          <Card.Description>
            查询近期开销和当前剩余电量；宿舍配置保存在本机安全存储，也可导出分享。
          </Card.Description>
        </Card.Header>
        <Card.Content>
          {(loading || loadingOptions) && (
            <div className="electricity-loading" aria-live="polite">
              <Spinner size="sm" aria-label="正在加载电费宿舍选项" />
              <span>正在加载宿舍选项…</span>
            </div>
          )}
          <Form className="electricity-form" onSubmit={submit}>
            {(!savedSettings || editing) && (
              <div className="electricity-fields">
                <LocationSelect
                  label="校区"
                  value={settings.campusId}
                  options={campuses}
                  disabled={formDisabled}
                  onChange={chooseCampus}
                />
                <LocationSelect
                  label="宿舍区域"
                  value={settings.areaId}
                  options={campus?.areas ?? []}
                  disabled={formDisabled || !campus}
                  onChange={chooseArea}
                />
                <LocationSelect
                  label="楼栋"
                  value={settings.buildingId}
                  options={buildings}
                  disabled={formDisabled || !area}
                  onChange={chooseBuilding}
                />
                <TextField
                  name="roomName"
                  isRequired
                  isInvalid={Boolean(settings.roomName.trim()) && roomFloor === null}
                >
                  <Label>宿舍号</Label>
                  <Input
                    disabled={formDisabled}
                    value={settings.roomName}
                    inputMode="numeric"
                    onChange={(e) => {
                      requestId.current += 1;
                      setResult(undefined);
                      setError(undefined);
                      setSettings((s) => ({ ...s, roomName: e.target.value }));
                    }}
                    placeholder="如 601 或 1201"
                  />
                  {settings.roomName.trim() && roomFloor === null && (
                    <FieldError>房间号应为一至两位楼层加两位房间号。</FieldError>
                  )}
                </TextField>
                {area && (
                  <TextField name="floorRange">
                    <Label>楼层范围</Label>
                    <Input
                      value={roomFloor === null ? "" : `${roomFloor} 层`}
                      readOnly
                      placeholder="输入宿舍号后自动识别"
                    />
                  </TextField>
                )}
              </div>
            )}
            {savedSettings && !editing && (
              <div className="electricity-location-summary">
                <span>当前宿舍</span>
                <strong>
                  {savedSettings.buildingName} {savedSettings.roomName}
                </strong>
              </div>
            )}
            {savedSettings && editing && (
              <Button
                type="button"
                variant="tertiary"
                isDisabled={busy || sharing}
                onPress={() => setSettings(savedSettings)}
              >
                使用上次配置
              </Button>
            )}
            <p className="text-sm text-muted">展示最近 31 天的用电与充值记录。</p>
            <div className="electricity-actions">
              <Button type="submit" variant="primary" isDisabled={formDisabled || !canQuery}>
                {busy && <Spinner size="sm" color="current" />}
                {busy ? "查询中…" : "查询电费"}
              </Button>
              {savedSettings && !editing && (
                <Button
                  type="button"
                  variant="outline"
                  isDisabled={formDisabled}
                  onPress={() => void refreshLatest()}
                >
                  刷新最新数据
                </Button>
              )}
              {savedSettings && (
                <Button
                  type="button"
                  variant="outline"
                  isDisabled={busy || sharing}
                  onPress={() => setEditing(!editing)}
                >
                  {editing ? "收起配置" : "修改宿舍配置"}
                </Button>
              )}
            </div>
          </Form>
          <div className="mt-4 flex flex-col gap-3">
            {savedSettings && (
              <Button
                variant="outline"
                isDisabled={formDisabled}
                onPress={() => void exportConfig()}
              >
                导出宿舍配置并复制
              </Button>
            )}
            {(!savedSettings || editing) && (
              <>
                <TextField value={importText} onChange={setImportText} isDisabled={formDisabled}>
                  <Label>宿舍配置码</Label>
                  <Input placeholder="粘贴分享的宿舍配置码" />
                </TextField>
                <Button
                  variant="secondary"
                  isDisabled={formDisabled || !importText.trim()}
                  onPress={() => void importConfig()}
                >
                  快捷导入宿舍配置
                </Button>
              </>
            )}
            {shareMessage && <p role="status">{shareMessage}</p>}
            {exportText && (
              <TextField value={exportText} isReadOnly>
                <Label>可复制的宿舍配置码</Label>
                <Input />
              </TextField>
            )}
          </div>
          {error && (
            <Alert status="danger" role="alert">
              <Alert.Indicator />
              <Alert.Content>
                <Alert.Title>查询未完成</Alert.Title>
                <Alert.Description>{error}</Alert.Description>
              </Alert.Content>
            </Alert>
          )}
        </Card.Content>
      </Card>
      {result && (
        <ElectricityDetails
          result={result}
          rate={rate}
          retryRecharges={() => void retryRecharges()}
          retrying={retrying}
        />
      )}
      {(loading || (busy && !result)) && (
        <div className="grid gap-4" role="status" aria-label="正在加载电费数据">
          <div className="grid gap-3 sm:grid-cols-2">
            {[0, 1, 2, 3].map((item) => (
              <SkeletonCard key={item} />
            ))}
          </div>
          <UiCard title="用电趋势">
            <div className="grid h-64 content-end gap-3" aria-hidden="true">
              <div className="h-40 animate-pulse rounded-lg bg-default-100" />
              <div className="h-3 w-2/3 animate-pulse rounded bg-default-100" />
            </div>
          </UiCard>
        </div>
      )}
    </div>
  );
}
