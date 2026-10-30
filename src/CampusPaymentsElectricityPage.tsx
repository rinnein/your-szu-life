import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, TextField, Button } from "material/react";
import { Banner, toast } from "./ui";
import { LuZap } from "react-icons/lu";
import { copyText } from "./clipboard";
import { ElectricityDetails } from "./ElectricityDetails";
import { Card as UiCard, SkeletonCard } from "./components";
import {
  getElectricityConstants,
  getElectricityRoomFloor,
  hasElectricityData,
  type ElectricitySettings as Settings,
  type ElectricityResult as Result,
  type ElectricityRecharge,
} from "./electricity";
import {
  ElectricityLocationConfirmDialog,
  ElectricityLocationFields,
  isElectricitySelectionAvailable,
  type ElectricityCampus as Campus,
} from "./electricity-location";

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
  const [confirmOpen, setConfirmOpen] = useState(false);
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
  const roomFloor = getElectricityRoomFloor(settings.roomName);
  const canQuery = Boolean(
    settings.campusId &&
    settings.areaId &&
    settings.buildingId &&
    settings.roomName.trim() &&
    roomFloor !== null &&
    isElectricitySelectionAvailable(campuses, settings),
  );
  const changeLocation = (next: Settings) => {
    requestId.current += 1;
    setResult(undefined);
    setError(undefined);
    setSettings(next);
  };
  const changeImportText = (event: any) =>
    setImportText(event.detail?.value ?? event.target?.value ?? "");
  const submit = async () => {
    if (!canQuery) {
      setConfirmOpen(false);
      return;
    }
    const id = ++requestId.current;
    setBusy(true);
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
      if (id === requestId.current) setBusy(false);
      setConfirmOpen(false);
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
    // 已保存的配置直接刷新即可，只有正在填写/修改的范围需要再确认一次。
    if (savedSettings && !editing) {
      void submit();
      return;
    }
    setConfirmOpen(true);
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
      <Card className="payments-card" variant="outlined">
        <div slot="headline" className="payments-card-title flex items-center gap-2">
          <LuZap aria-hidden="true" />
          <span>查电量</span>
        </div>
        <div slot="subhead">
          查询近期开销和当前剩余电量；宿舍配置保存在本机安全存储，也可导出分享。
        </div>
        <div>
          {(loading || loadingOptions) && (
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
            {(!savedSettings || editing) && (
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
                disabled={busy || sharing}
                onClick={() => setSettings(savedSettings)}
              >
                使用上次配置
              </Button>
            )}
            <p className="text-sm text-muted">展示最近 31 天的用电与充值记录。</p>
            <div className="electricity-actions">
              <Button type="submit" variant="filled" disabled={formDisabled || !canQuery}>
                {busy && <LoadingIndicator size={16} />}
                {busy ? "查询中…" : "查询电费"}
              </Button>
              {savedSettings && !editing && (
                <Button
                  type="button"
                  variant="outlined"
                  disabled={formDisabled}
                  onClick={() => void refreshLatest()}
                >
                  刷新最新数据
                </Button>
              )}
              {savedSettings && (
                <Button
                  type="button"
                  variant="outlined"
                  disabled={busy || sharing}
                  onClick={() => setEditing(!editing)}
                >
                  {editing ? "收起配置" : "修改宿舍配置"}
                </Button>
              )}
            </div>
          </form>
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
            {(!savedSettings || editing) && (
              <>
                <TextField
                  label="宿舍配置码"
                  value={importText}
                  onInput={changeImportText}
                  onChange={changeImportText}
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
            {shareMessage && <p role="status">{shareMessage}</p>}
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
      <ElectricityLocationConfirmDialog
        open={confirmOpen}
        campuses={campuses}
        value={settings}
        busy={busy}
        confirmLabel="确认并查询"
        onClose={() => {
          if (!busy) setConfirmOpen(false);
        }}
        onConfirm={() => void submit()}
      />
    </div>
  );
}
