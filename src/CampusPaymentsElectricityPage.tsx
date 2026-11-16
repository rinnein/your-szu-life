import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, TextField, Button } from "material/react";
import { Banner, bindField, toast } from "./ui";
import { LuZap } from "react-icons/lu";
import { copyText } from "./clipboard";
import { ElectricityDetails } from "./ElectricityDetails";
import { Card as UiCard, EmptyState, SkeletonCard } from "./components";
import {
  getElectricityConstants,
  getElectricityRoomFloor,
  hasElectricityData,
  type ElectricitySettings as Settings,
  type ElectricityResult as Result,
  type ElectricityRecharge,
} from "./electricity";
import {
  ElectricityLocationFields,
  isElectricitySelectionAvailable,
  type ElectricityCampus as Campus,
} from "./electricity-location";

/** 宿舍选项加载：未需要（已存配置且未编辑）/ 加载中 / 就绪 / 失败（可重试）。 */
type OptionsState =
  | { status: "hidden" }
  | { status: "loading" }
  | { status: "ready" }
  | { status: "error"; message: string };
/** 数据操作占用：查询（含保存）/ 刷新（非强制）/ 强制刷新，转圈只落在发起动作的按钮上。 */
type Pending = null | "query" | "refresh" | "force";

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
  const [options, setOptions] = useState<OptionsState>({ status: "hidden" });
  const [pending, setPending] = useState<Pending>(null);
  const [editing, setEditing] = useState(false);
  const [rate, setRate] = useState<number>();
  const [rateError, setRateError] = useState(false);
  const [savedSettings, setSavedSettings] = useState<Settings>();
  const [importText, setImportText] = useState("");
  const [exportText, setExportText] = useState("");
  const [sharing, setSharing] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const [optionsAttempt, setOptionsAttempt] = useState(0);
  const requestId = useRef(0);
  useEffect(() => {
    let active = true;
    void getElectricityConstants()
      .then((constants) => {
        if (active) setRate(constants?.yuanPerKwh);
      })
      .catch(() => {
        if (active) setRateError(true);
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
        const stored = await initialSettingsRequest.current;
        const saved = stored && { ...stored, roomName: stored.roomName.trim() };
        if (!active) return;
        if (!saved) {
          setLoading(false);
          return;
        }
        setSettings(saved);
        if (getElectricityRoomFloor(saved.roomName) === null) {
          setEditing(true);
          setError("已保存的宿舍号不符合当前格式，请重新填写后查询。");
          setLoading(false);
          return;
        }
        setSavedSettings(saved);
        setLoading(false);
        setPending("query");
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
          if (active && id === requestId.current) setPending(null);
        }
      } catch (e) {
        if (active) {
          setError(typeof e === "string" ? e : "无法读取宿舍配置。");
          setPending(null);
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
    const needed = !loading && (!savedSettings || editing);
    if (!needed) {
      setOptions({ status: "hidden" });
      return;
    }
    let active = true;
    setOptions({ status: "loading" });
    optionsRequest.current ??= invoke<Campus[]>("get_electricity_options");
    void optionsRequest.current
      .then((next) => {
        if (active) {
          setCampuses(Array.isArray(next) ? next : []);
          setOptions({ status: "ready" });
        }
      })
      .catch((e) => {
        optionsRequest.current = undefined;
        if (active)
          setOptions({
            status: "error",
            message: typeof e === "string" ? e : "无法加载宿舍选项，请连接校园网后重试。",
          });
      });
    return () => {
      active = false;
    };
  }, [loading, savedSettings, editing, optionsAttempt]);
  const roomFloor = getElectricityRoomFloor(settings.roomName);
  const optionsNeeded = !savedSettings || editing;
  const canQuery = Boolean(
    settings.campusId &&
    settings.areaId &&
    settings.buildingId &&
    settings.roomName.trim() &&
    roomFloor !== null &&
    isElectricitySelectionAvailable(campuses, settings),
  );
  const formDisabled =
    loading || pending !== null || sharing || (optionsNeeded && options.status !== "ready");
  // 修改草稿时保留上次查询结果供参照（结果区标明所属宿舍），仅作废在途请求。
  const changeLocation = (next: Settings) => {
    requestId.current += 1;
    setError(undefined);
    setSettings(next);
  };
  const restoreSaved = () => {
    if (!savedSettings) return;
    requestId.current += 1;
    setSettings(savedSettings);
  };
  const submit = async () => {
    if (!canQuery || pending) return;
    const id = ++requestId.current;
    setPending("query");
    setError(undefined);
    try {
      const normalized = { ...settings, roomName: settings.roomName.trim() };
      // 先确认能查到电费信息，再保存宿舍配置。
      const next = await invoke<Result>("query_electricity", { input: normalized });
      if (id !== requestId.current) return;
      if (!hasElectricityData(next)) {
        setError("未查询到该宿舍的电费信息，请检查宿舍信息后重试。配置尚未保存。");
        toast.warning("未查询到该宿舍的电费信息", {
          description: "配置尚未保存，请检查校区、楼栋和宿舍号后重试。",
        });
        return;
      }
      await invoke("save_electricity_settings", { settings: normalized });
      if (id !== requestId.current) return;
      setSettings(normalized);
      setSavedSettings(normalized);
      setResult(next);
      setEditing(false);
      setError(next.cache?.refreshError || undefined);
      if (next.cache?.refreshError) {
        toast.warning("电费刷新未完成", { description: next.cache.refreshError });
      }
    } catch (e) {
      if (id === requestId.current) {
        const msg = typeof e === "string" ? e : "电费查询失败，请检查宿舍信息和校园网连接。";
        setError(msg);
        toast.danger("查询未完成，配置尚未保存", { description: msg });
      }
    } finally {
      if (id === requestId.current) setPending(null);
    }
  };
  const requestSubmit = () => {
    if (!canQuery) {
      const msg =
        settings.roomName.trim() && roomFloor === null
          ? "宿舍号需为一至两位楼层加两位房间号，例如 601 或 1201。"
          : "请先选择校区、宿舍区域、楼栋并填写宿舍号。";
      setError(msg);
      toast.warning(msg);
      return;
    }
    void submit();
  };
  // 非强制刷新走后端缓存策略（每天 23:59 左右更新）；强制刷新直连门户，
  // 只在结果区提供，避免与表单区的主操作重复。
  const refreshLatest = async (forceRefresh: boolean) => {
    if (!savedSettings || pending) return;
    const kind = forceRefresh ? "force" : "refresh";
    const id = ++requestId.current;
    setPending(kind);
    setError(undefined);
    try {
      const next = await invoke<Result>("query_electricity", {
        input: savedSettings,
        forceRefresh,
      });
      if (id === requestId.current) {
        setResult(next);
        setError(next.cache?.refreshError || undefined);
      }
    } catch (e) {
      if (id === requestId.current)
        setError(typeof e === "string" ? e : "更新未完成，已有记录仍可查看。");
    } finally {
      if (id === requestId.current) setPending(null);
    }
  };
  const exportConfig = async () => {
    if (sharing) return;
    setSharing(true);
    setExportText("");
    try {
      const text = await invoke<string>("export_electricity_settings");
      if (await copyText(text)) toast.success("宿舍配置已复制");
      else {
        setExportText(text);
        toast.warning("无法自动复制，请在下方配置码手动复制");
      }
    } catch (e) {
      toast.danger("导出失败", { description: typeof e === "string" ? e : undefined });
    } finally {
      setSharing(false);
    }
  };
  const importConfig = async () => {
    if (sharing) return;
    setSharing(true);
    try {
      const imported = await invoke<Settings>("import_electricity_settings", { text: importText });
      const normalized = { ...imported, roomName: imported.roomName.trim() };
      requestId.current += 1;
      setError(undefined);
      setSettings(normalized);
      setEditing(true);
      toast.success("已填入宿舍配置，点击查询电费保存并查询");
    } catch (e) {
      toast.danger("导入失败，请检查配置码", {
        description: typeof e === "string" ? e : undefined,
      });
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
  const retryOptions = () => {
    optionsRequest.current = undefined;
    setOptionsAttempt((n) => n + 1);
  };

  return (
    <div className="electricity-page">
      <Card className="payments-card" variant="outlined">
        <div slot="headline" className="payments-card-title flex items-center gap-2">
          <LuZap aria-hidden="true" />
          <span>查电量</span>
        </div>
        <div slot="subhead">
          查询近期开销和当前剩余电量；宿舍配置保存在本机安全存储，也可导出分享。
        </div>
        <div>
          {loading && (
            <div className="electricity-loading" aria-live="polite">
              <LoadingIndicator size={16} aria-label="正在读取宿舍配置" />
              <span>正在读取宿舍配置…</span>
            </div>
          )}
          {options.status === "loading" && (
            <div className="electricity-loading" aria-live="polite">
              <LoadingIndicator size={16} aria-label="正在加载电费宿舍选项" />
              <span>正在加载宿舍选项…</span>
            </div>
          )}
          <form
            className="electricity-form"
            onSubmit={(e) => {
              e.preventDefault();
              requestSubmit();
            }}
          >
            {optionsNeeded && (
              <ElectricityLocationFields
                campuses={campuses}
                value={settings}
                disabled={formDisabled}
                onChange={changeLocation}
              />
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
                variant="text"
                disabled={pending !== null || sharing}
                onClick={restoreSaved}
              >
                使用上次配置
              </Button>
            )}
            <p className="text-sm text-muted">展示最近 31 天的用电与充值记录。</p>
            <div className="electricity-actions">
              {optionsNeeded && (
                <Button type="submit" variant="filled" disabled={formDisabled || !canQuery}>
                  {pending === "query" && <LoadingIndicator size={16} />}
                  查询电费
                </Button>
              )}
              {savedSettings && !editing && (
                <Button
                  type="button"
                  variant="filled"
                  disabled={formDisabled}
                  onClick={() => void refreshLatest(false)}
                >
                  {pending === "refresh" && <LoadingIndicator size={16} />}
                  刷新数据
                </Button>
              )}
              {savedSettings && (
                <Button
                  type="button"
                  variant="outlined"
                  disabled={pending !== null || sharing}
                  onClick={() => {
                    // 收起时恢复已保存配置：草稿不会与已存配置静默分叉，
                    // 结果区的数据也随之与配置一致。
                    if (editing) restoreSaved();
                    setEditing(!editing);
                  }}
                >
                  {editing ? "收起配置" : "修改宿舍配置"}
                </Button>
              )}
            </div>
          </form>
          {options.status === "error" && optionsNeeded && (
            <Banner status="danger" className="mt-4">
              <Banner.Title>宿舍选项加载失败</Banner.Title>
              <Banner.Description>{options.message}</Banner.Description>
              <Button variant="text" onClick={retryOptions}>
                重试
              </Button>
            </Banner>
          )}
          <div className="mt-4 flex flex-col gap-3">
            {savedSettings && (
              <Button
                variant="outlined"
                disabled={formDisabled}
                onClick={() => void exportConfig()}
              >
                导出宿舍配置并复制
              </Button>
            )}
            {optionsNeeded && (
              <>
                <TextField
                  label="宿舍配置码"
                  value={importText}
                  {...bindField(setImportText)}
                  disabled={formDisabled}
                  placeholder="粘贴分享的宿舍配置码"
                />
                <Button
                  variant="tonal"
                  disabled={formDisabled || !importText.trim()}
                  onClick={() => void importConfig()}
                >
                  快捷导入宿舍配置
                </Button>
              </>
            )}
            {exportText && <TextField label="可复制的宿舍配置码" value={exportText} readonly />}
          </div>
          {error && (
            <Banner status="danger" role="alert">
              <Banner.Title>查询未完成</Banner.Title>
              <Banner.Description>{error}</Banner.Description>
            </Banner>
          )}
        </div>
      </Card>
      {result && (
        <ElectricityDetails
          result={result}
          rate={rate}
          rateError={rateError}
          dormLabel={
            savedSettings ? `${savedSettings.buildingName} ${savedSettings.roomName}` : undefined
          }
          onForceRefresh={savedSettings ? () => void refreshLatest(true) : undefined}
          refreshing={pending === "force"}
          retryRecharges={() => void retryRecharges()}
          retrying={retrying}
        />
      )}
      {(loading || ((pending === "query" || pending === "refresh") && !result)) && (
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
      {savedSettings && !editing && !result && !error && !loading && pending === null && (
        <EmptyState
          title="尚未查询"
          description="点击「刷新数据」获取最近 31 天的用电与充值记录。"
        />
      )}
    </div>
  );
}
