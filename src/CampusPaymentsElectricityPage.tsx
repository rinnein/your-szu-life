import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Input, Label, Spinner, TextField } from "@heroui/react";
import { LuZap } from "react-icons/lu";

type Building = { id: string; name: string };
type Area = { id: string; name: string; campusId: string; provider: string; buildings: Building[] };
type Campus = { id: string; name: string; areas: Area[] };
type Settings = {
  campusId: string;
  areaId: string;
  buildingId: string;
  buildingName: string;
  roomName: string;
};
type Reading = {
  date: string;
  roomName: string;
  remaining: number;
  totalUsed: number;
  totalPurchased: number;
  dailyUsed?: number;
  dailyPurchased?: number;
};
type Result = {
  campusId: string;
  areaId: string;
  buildingName: string;
  roomName: string;
  beginDate: string;
  endDate: string;
  readings: Reading[];
  currentRemaining?: number;
};

function dateOffset(days: number) {
  const d = new Date();
  d.setDate(d.getDate() + days);
  return d.toISOString().slice(0, 10);
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
  const [beginDate, setBeginDate] = useState(dateOffset(-6));
  const [endDate, setEndDate] = useState(dateOffset(0));
  const [result, setResult] = useState<Result>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState(false);
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
        }
        // Render the configured page first, then refresh it asynchronously just
        // like a user submitting the query form.
        if (active) setLoading(false);
        if (saved) {
          setBusy(true);
          void invoke<Result>("query_electricity", {
            input: { ...saved, beginDate, endDate },
          })
            .then((automatic) => {
              if (active) {
                setResult(automatic);
                setEditing(false);
              }
            })
            .catch((e) => {
              if (active) {
                setError(
                  typeof e === "string" ? e : "无法自动获取当前电量，请检查宿舍信息和校园网连接。",
                );
              }
            })
            .finally(() => {
              if (active) setBusy(false);
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
    };
  }, []);
  const campus = campuses.find((x) => x.id === settings.campusId);
  const area = campus?.areas.find((x) => x.id === settings.areaId);
  const buildings = area?.buildings ?? [];
  const canQuery = Boolean(
    settings.campusId && settings.areaId && settings.buildingId && settings.roomName.trim(),
  );
  const chooseCampus = (campusId: string) => {
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
    const next = buildings.find((x) => x.id === buildingId);
    setSettings((s) => ({ ...s, buildingId, buildingName: next?.name ?? "" }));
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!canQuery) {
      setError("请先选择校区、宿舍区域、楼栋并填写宿舍号。");
      return;
    }
    setBusy(true);
    setError(undefined);
    try {
      await invoke("save_electricity_settings", { settings });
      const next = await invoke<Result>("query_electricity", {
        input: { ...settings, beginDate, endDate },
      });
      setResult(next);
      setEditing(false);
    } catch (e) {
      setError(typeof e === "string" ? e : "电费查询失败，请检查宿舍信息和校园网连接。");
    } finally {
      setBusy(false);
    }
  };
  const currentRemaining = result?.currentRemaining ?? result?.readings.slice(-1)[0]?.remaining;
  const formDisabled = loading || busy;
  return (
    <div className="electricity-page">
      <Card className="payments-card">
        <Card.Header>
          <Card.Title className="payments-card-title">
            <LuZap aria-hidden="true" />
            查电量
          </Card.Title>
          <Card.Description>
            查询近期开销和当前剩余电量；宿舍配置仅保存在本机安全存储。
          </Card.Description>
        </Card.Header>
        <Card.Content>
          {loading && (
            <div className="electricity-loading" aria-live="polite">
              <Spinner size="sm" aria-label="正在加载电费宿舍选项" />
              <span>正在加载宿舍选项…</span>
            </div>
          )}
          <form className="electricity-form" onSubmit={submit}>
            {(!result || editing) && (
              <div className="electricity-fields">
                <label>
                  校区
                  <select
                    value={settings.campusId}
                    onChange={(e) => chooseCampus(e.target.value)}
                    disabled={formDisabled}
                  >
                    <option value="">请选择校区</option>
                    {campuses.map((x) => (
                      <option key={x.id} value={x.id}>
                        {x.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  宿舍区域
                  <select
                    value={settings.areaId}
                    onChange={(e) => chooseArea(e.target.value)}
                    disabled={formDisabled || !campus}
                  >
                    <option value="">请选择区域</option>
                    {campus?.areas.map((x) => (
                      <option key={x.id} value={x.id}>
                        {x.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  楼栋
                  <select
                    value={settings.buildingId}
                    onChange={(e) => chooseBuilding(e.target.value)}
                    disabled={formDisabled || !area}
                  >
                    <option value="">请选择楼栋</option>
                    {buildings.map((x) => (
                      <option key={x.id} value={x.id}>
                        {x.name}
                      </option>
                    ))}
                  </select>
                </label>
                <TextField name="roomName" isRequired>
                  <Label>宿舍号</Label>
                  <Input
                    disabled={formDisabled}
                    value={settings.roomName}
                    onChange={(e) => setSettings((s) => ({ ...s, roomName: e.target.value }))}
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
            <div className="electricity-dates">
              <TextField name="beginDate" isRequired>
                <Label>开始日期</Label>
                <Input
                  disabled={formDisabled}
                  type="date"
                  value={beginDate}
                  onChange={(e) => setBeginDate(e.target.value)}
                />
              </TextField>
              <TextField name="endDate" isRequired>
                <Label>结束日期</Label>
                <Input
                  disabled={formDisabled}
                  type="date"
                  value={endDate}
                  onChange={(e) => setEndDate(e.target.value)}
                />
              </TextField>
            </div>
            <div className="electricity-actions">
              <Button type="submit" variant="primary" isDisabled={formDisabled || !canQuery}>
                {busy && <Spinner size="sm" color="current" />}
                {busy ? "查询中…" : "查询电费"}
              </Button>
              {result && (
                <Button type="button" variant="outline" onPress={() => setEditing(!editing)}>
                  {editing ? "收起配置" : "修改宿舍配置"}
                </Button>
              )}
            </div>
          </form>
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
        <Card className="payments-card electricity-results">
          <Card.Header>
            <Card.Title>电费查询结果</Card.Title>
            <Card.Description>
              {result.buildingName} {result.roomName} · {result.beginDate} 至 {result.endDate}
            </Card.Description>
          </Card.Header>
          <Card.Content>
            <div className="electricity-current" aria-label="当前电量">
              <span className="electricity-current-label">当前电量</span>
              <strong>
                {currentRemaining == null ? "暂无数据" : `${currentRemaining.toFixed(2)} 度`}
              </strong>
            </div>
            <h3 className="electricity-section-title">用电记录</h3>
            {result.readings.length === 0 ? (
              <p className="payments-empty">该时间段没有查询到记录。</p>
            ) : (
              <div className="electricity-table-wrap">
                <table className="electricity-table">
                  <thead>
                    <tr>
                      <th>日期</th>
                      <th>剩余电量</th>
                      <th>总用电量</th>
                      <th>总购电量</th>
                      <th>当日用电</th>
                      <th>当日购电</th>
                    </tr>
                  </thead>
                  <tbody>
                    {result.readings.map((row) => (
                      <tr key={row.date}>
                        <td>{row.date}</td>
                        <td>{row.remaining.toFixed(2)}</td>
                        <td>{row.totalUsed.toFixed(2)}</td>
                        <td>{row.totalPurchased.toFixed(2)}</td>
                        <td>{row.dailyUsed == null ? "—" : row.dailyUsed.toFixed(2)}</td>
                        <td>{row.dailyPurchased == null ? "—" : row.dailyPurchased.toFixed(2)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Card.Content>
        </Card>
      )}
    </div>
  );
}
