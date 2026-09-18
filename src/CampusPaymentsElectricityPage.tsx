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
  Popover,
  RangeCalendar,
  Select,
  Spinner,
  Table,
  TextField,
} from "@heroui/react";
import { getLocalTimeZone, today, type CalendarDate } from "@internationalized/date";
import type { RangeValue } from "react-aria-components";
import { LuZap } from "react-icons/lu";
import { EMPTY_ELECTRICITY_STATUS, useElectricityMonitor } from "./electricity-monitor";

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

function initialDateRange(): RangeValue<CalendarDate> {
  const end = today(getLocalTimeZone());
  return { start: end.subtract({ days: 6 }), end };
}

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
  const monitor = useElectricityMonitor() ?? EMPTY_ELECTRICITY_STATUS;
  const [campuses, setCampuses] = useState<Campus[]>([]);
  const [settings, setSettings] = useState<Settings>({
    campusId: "",
    areaId: "",
    buildingId: "",
    buildingName: "",
    roomName: "",
  });
  const [dateRange, setDateRange] = useState<RangeValue<CalendarDate>>(initialDateRange);
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
            input: {
              ...saved,
              beginDate: dateRange.start.toString(),
              endDate: dateRange.end.toString(),
            },
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
  const submit = async (event: FormEvent<HTMLFormElement>) => {
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
        input: {
          ...settings,
          beginDate: dateRange.start.toString(),
          endDate: dateRange.end.toString(),
        },
      });
      setResult(next);
      setEditing(false);
    } catch (e) {
      setError(typeof e === "string" ? e : "电费查询失败，请检查宿舍信息和校园网连接。");
    } finally {
      setBusy(false);
    }
  };
  const currentRemaining = monitor.remaining ?? result?.currentRemaining;
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
              <Popover>
                <Popover.Trigger>
                  <Button
                    type="button"
                    variant="outline"
                    isDisabled={formDisabled}
                    aria-label="查询日期范围"
                  >
                    {dateRange.start.toString()} 至 {dateRange.end.toString()}
                  </Button>
                </Popover.Trigger>
                <Popover.Content>
                  <RangeCalendar
                    aria-label="查询日期范围"
                    value={dateRange}
                    onChange={(next) => next && setDateRange(next)}
                    firstDayOfWeek="mon"
                  >
                    <RangeCalendar.Header>
                      <RangeCalendar.NavButton slot="previous" />
                      <RangeCalendar.Heading />
                      <RangeCalendar.NavButton slot="next" />
                    </RangeCalendar.Header>
                    <RangeCalendar.Grid>
                      <RangeCalendar.GridHeader>
                        {(_day) => <RangeCalendar.HeaderCell />}
                      </RangeCalendar.GridHeader>
                      <RangeCalendar.GridBody>
                        {(date) => <RangeCalendar.Cell date={date} />}
                      </RangeCalendar.GridBody>
                    </RangeCalendar.Grid>
                  </RangeCalendar>
                </Popover.Content>
              </Popover>
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
          </Form>
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
              {monitor.expired && currentRemaining != null && (
                <small className="electricity-current-stale">数据将在下次查询后更新</small>
              )}
            </div>
            <h3 className="electricity-section-title">用电记录</h3>
            {result.readings.length === 0 ? (
              <p className="payments-empty">该时间段没有查询到记录。</p>
            ) : (
              <Table aria-label="用电记录">
                <Table.ScrollContainer>
                  <Table.Content aria-label="用电记录">
                    <Table.Header>
                      <Table.Column id="date" isRowHeader>
                        日期
                      </Table.Column>
                      <Table.Column id="remaining">剩余电量</Table.Column>
                      <Table.Column id="dailyUsed">当日用电</Table.Column>
                      <Table.Column id="dailyPurchased">当日购电</Table.Column>
                    </Table.Header>
                    <Table.Body items={result.readings}>
                      {(row) => (
                        <Table.Row id={row.date}>
                          <Table.Cell>{row.date}</Table.Cell>
                          <Table.Cell>{row.remaining.toFixed(2)}</Table.Cell>
                          <Table.Cell>
                            {row.dailyUsed == null ? "—" : row.dailyUsed.toFixed(2)}
                          </Table.Cell>
                          <Table.Cell>
                            {row.dailyPurchased == null ? "—" : row.dailyPurchased.toFixed(2)}
                          </Table.Cell>
                        </Table.Row>
                      )}
                    </Table.Body>
                  </Table.Content>
                </Table.ScrollContainer>
              </Table>
            )}
          </Card.Content>
        </Card>
      )}
    </div>
  );
}
