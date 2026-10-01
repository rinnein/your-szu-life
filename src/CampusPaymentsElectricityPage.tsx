import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Alert,
  Button,
  Card,
  Form,
  Input,
  Label,
  ListBox,
  Select,
  Spinner,
  TextField,
} from "@heroui/react";
import { LuZap } from "react-icons/lu";
import { copyText } from "./clipboard";
import { ElectricityDetails } from "./ElectricityDetails";
import {
  getElectricityConstants,
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
  const initialDataRequest = useRef<Promise<[Campus[], Settings | null]> | undefined>(undefined);
  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        initialDataRequest.current ??= Promise.all([
          invoke<Campus[]>("get_electricity_options"),
          invoke<Settings | null>("get_electricity_settings"),
        ]);
        const [next, saved] = await initialDataRequest.current;
        if (!active) return;
        setCampuses(Array.isArray(next) ? next : []);
        if (saved) {
          setSettings(saved);
          setSavedSettings(saved);
        }
        // Render the configured page first, then refresh it asynchronously just
        // like a user submitting the query form.
        if (active) setLoading(false);
        if (saved) {
          setBusy(true);
          const id = ++requestId.current;
          void invoke<Result>("query_electricity", {
            input: {
              ...saved,
            },
          })
            .then((automatic) => {
              if (active && id === requestId.current) {
                setResult(automatic);
                setEditing(false);
              }
            })
            .catch((e) => {
              if (active && id === requestId.current) {
                setError(
                  typeof e === "string" ? e : "无法自动获取当前电量，请检查宿舍信息和校园网连接。",
                );
              }
            })
            .finally(() => {
              if (active && id === requestId.current) setBusy(false);
            });
        }
      } catch (e) {
        if (active) {
          setError(typeof e === "string" ? e : "无法读取电费宿舍选项，请连接校园网后重试。");
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
  const campus = campuses.find((x) => x.id === settings.campusId);
  const area = campus?.areas.find((x) => x.id === settings.areaId);
  const buildings = area?.buildings ?? [];
  const canQuery = Boolean(
    settings.campusId && settings.areaId && settings.buildingId && settings.roomName.trim(),
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
      setError("请先选择校区、宿舍区域、楼栋并填写宿舍号。");
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
      }
    } catch (e) {
      if (id === requestId.current)
        setError(typeof e === "string" ? e : "电费查询失败，请检查宿舍信息和校园网连接。");
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
  const formDisabled = loading || busy || sharing;
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
          {loading && (
            <div className="electricity-loading" aria-live="polite">
              <Spinner size="sm" aria-label="正在加载电费宿舍选项" />
              <span>正在加载宿舍选项…</span>
            </div>
          )}
          <Form className="electricity-form" onSubmit={submit}>
            {(!result || editing) && (
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
                <TextField name="roomName" isRequired>
                  <Label>宿舍号</Label>
                  <Input
                    disabled={formDisabled}
                    value={settings.roomName}
                    onChange={(e) => {
                      requestId.current += 1;
                      setResult(undefined);
                      setSettings((s) => ({ ...s, roomName: e.target.value }));
                    }}
                    placeholder="如 601"
                  />
                </TextField>
              </div>
            )}
            {result && !editing && (
              <div className="electricity-location-summary">
                <span>当前宿舍</span>
                <strong>
                  {result.buildingName} {result.roomName}
                </strong>
              </div>
            )}
            <p className="text-sm text-muted">展示最近 31 天的用电与充值记录。</p>
            <div className="electricity-actions">
              <Button type="submit" variant="primary" isDisabled={formDisabled || !canQuery}>
                {busy && <Spinner size="sm" color="current" />}
                {busy ? "查询中…" : "查询电费"}
              </Button>
              {result && (
                <Button
                  type="button"
                  variant="outline"
                  isDisabled={formDisabled || sharing}
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
            {(!result || editing) && (
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
    </div>
  );
}
