import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Card, Dialog, LoadingIndicator, Switch, TextField, Button } from "material/react";
import { AdaptiveTabs, Banner, bindField, toast } from "./ui";
import { EmptyState, LoadingState, ModuleHeader, PageShell } from "./components";
import { usePending } from "./use-pending";
import { errorMessage } from "./async-state";
import StaffNetworkPanel from "./StaffNetworkPanel";
import {
  initialStaffSettings,
  initialStaffStatus,
  type StaffSettings,
  type StaffStatus,
  type StaffSettingsInput,
  type StaffApplyResult,
} from "./staff-network";

type CredentialMode = "currentAccount" | "custom";
type RegionTab = "staff" | "dormitory";
type NetworkRegion = "none" | "staff" | "dormitory";
type NetworkState = "noNetwork" | "unknown" | "notLoggedIn" | "connected";
type Settings = {
  enabled: boolean;
  credentialMode: CredentialMode;
  customConfigured: boolean;
  customCampusCardNumber?: string | null;
};
type PortalSession = {
  campusCardNumber?: string | null;
  onlineUsername?: string | null;
  onlineDuration?: number | null;
  onlineFlow?: number | null;
  loginIp?: string | null;
  loginStartedAt?: string | null;
  loginExpiresAt?: string | null;
  charset?: string | null;
};
type StaffSession = {
  username: string;
  realName?: string | null;
  loginIp: string;
  loginIpv6?: string | null;
  userMac?: string | null;
  usedBytes?: number | null;
  usedSeconds?: number | null;
  loginStartedAtMs?: number | null;
};
type LogoutResult = {
  outcome: "disconnected" | "alreadyOffline" | "failed" | "unknown";
  region: NetworkRegion;
  message: string;
};
type NetworkOverview = {
  state: NetworkState;
  connected?: boolean;
  region: NetworkRegion;
  ssid?: string | null;
  portalReachable: boolean;
  staffHostReachable: boolean;
  portalSession?: PortalSession | null;
  staffSession?: StaffSession | null;
};
type Status = {
  state: "disabled" | "monitoring" | "probing" | "loggingIn" | "online" | "paused" | "error";
  ssid?: string | null;
  connected: boolean;
  portalReachable: boolean;
  consecutiveFailures: number;
  nextProbeAtMs?: number | null;
  lastResult?: { ok: boolean; alreadyOnline: boolean; message?: string | null } | null;
};

/** 页面初始加载：加载中 / 就绪 / 失败（失败展示 EmptyState 并可重试）。 */
type LoadState = { status: "loading" } | { status: "ready" } | { status: "error"; message: string };
/** 断开连接状态机：确认 → 执行中（不可关闭）→ 失败（可重试/关闭）；成功自动关闭。 */
type DisconnectState =
  | null
  | { step: "confirm" }
  | { step: "running" }
  | { step: "failed"; title: string; message: string };
/** 宿舍区操作占用：保存与手动登录各自独立转圈。 */
type DormPending = null | "save" | "login";

/** 宿舍区与教工区共用同一组登录状态枚举；子标题不再直接打印后端枚举。 */
const loginStatusText: Record<Status["state"], string> = {
  disabled: "自动登录未启用",
  monitoring: "自动登录监控中",
  probing: "正在探测门户",
  loggingIn: "正在登录",
  online: "在线",
  paused: "已暂停",
  error: "异常",
};

const initialOverview: NetworkOverview = {
  state: "noNetwork",
  connected: false,
  region: "none",
  portalReachable: false,
  staffHostReachable: false,
};
const initialStatus: Status = {
  state: "disabled",
  connected: false,
  portalReachable: false,
  consecutiveFailures: 0,
};

function regionLabel(region: NetworkRegion) {
  return region === "staff" ? "教工区" : region === "dormitory" ? "宿舍区" : "未识别区域";
}

function statusText(overview: NetworkOverview) {
  if (overview.state === "noNetwork") return "无网络";
  const region = regionLabel(overview.region);
  if (overview.state === "unknown") {
    return overview.connected === true ? "已连接(" + region + ")" : "状态待确认(" + region + ")";
  }
  return (overview.state === "connected" ? "已连接(" : "未登录(") + region + ")";
}

function formatTime(value?: number | null) {
  if (!value) return "暂未安排";
  return new Date(value).toLocaleTimeString();
}

function formatPortalDuration(value?: number | null) {
  return value === 4294967295 ? "未知/不限" : formatUsedSeconds(value);
}

function formatPortalFlow(value?: number | null) {
  return value === 4294967295 ? "未知/不限" : formatUsedBytes(value);
}

function formatUsedBytes(value?: number | null) {
  if (value == null || !Number.isFinite(value) || value < 0) return "网关未提供";
  const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  const index =
    value === 0 ? 0 : Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  return (
    (value / 1024 ** index).toLocaleString("zh-CN", { maximumFractionDigits: 2 }) +
    " " +
    units[index]
  );
}

function formatUsedSeconds(value?: number | null) {
  if (value == null || !Number.isFinite(value) || value < 0) return "网关未提供";
  let remaining = Math.floor(value);
  const parts: string[] = [];
  for (const [unit, size] of [
    ["天", 86400],
    ["小时", 3600],
    ["分", 60],
    ["秒", 1],
  ] as const) {
    const count = Math.floor(remaining / size);
    remaining %= size;
    if (count || (size === 1 && parts.length === 0)) parts.push(count + " " + unit);
  }
  return parts.join(" ");
}

function formatLoginStartedAt(value?: number | null) {
  if (value == null || !Number.isFinite(value)) return "网关未提供";
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "网关未提供"
    : date.toLocaleString("zh-CN", { hour12: false });
}

function regionDefaultTab(region: NetworkRegion): RegionTab {
  return region === "staff" ? "staff" : "dormitory";
}

export default function CampusNetworkPage() {
  const [load, setLoad] = useState<LoadState>({ status: "loading" });
  const [overview, setOverview] = useState<NetworkOverview>(initialOverview);
  const [settings, setSettings] = useState<Settings>();
  const [status, setStatus] = useState<Status>(initialStatus);
  const [staffSettings, setStaffSettings] = useState<StaffSettings>(initialStaffSettings);
  const [staffStatus, setStaffStatus] = useState<StaffStatus>(initialStaffStatus);
  const [regionTab, setRegionTab] = useState<RegionTab>("dormitory");
  const [mode, setMode] = useState<CredentialMode>("currentAccount");
  const [enabled, setEnabled] = useState(false);
  const [card, setCard] = useState("");
  const [password, setPassword] = useState("");
  const [dormPending, setDormPending] = useState<DormPending>(null);
  /** 页面级持久错误：仅来自初始加载之外的连接刷新与事件订阅失败；随下次成功刷新清除。 */
  const [error, setError] = useState<string>();
  const [disconnect, setDisconnect] = useState<DisconnectState>(null);
  const disconnectPending = usePending();
  const [refreshBusy, setRefreshBusy] = useState(false);
  const initializedTab = useRef(false);
  const mounted = useRef(false);
  const refreshBusyRef = useRef(false);
  const revisions = useRef({
    overview: 0,
    settings: 0,
    status: 0,
    staffSettings: 0,
    staffStatus: 0,
    refresh: 0,
  });
  const operationBusy =
    dormPending !== null ||
    disconnect?.step === "running" ||
    disconnectPending.isPending("disconnect");
  const staffInProgress = staffStatus.state === "probing" || staffStatus.state === "loggingIn";
  const loginInProgress =
    status.state === "probing" || status.state === "loggingIn" || staffInProgress;

  const applyOverview = (next: NetworkOverview) => {
    setOverview(next);
    if (!initializedTab.current && next.region !== "none") {
      setRegionTab(regionDefaultTab(next.region));
      initializedTab.current = true;
    }
  };
  const applySettings = (next: Settings) => {
    setSettings(next);
    setEnabled(next.enabled);
    setMode(next.credentialMode);
    setCard(next.customCampusCardNumber ?? "");
  };

  const refreshConnection = async (showError = true) => {
    if (refreshBusyRef.current) return;
    refreshBusyRef.current = true;
    setRefreshBusy(true);
    if (showError) setError(undefined);
    const overviewRevision = revisions.current.overview;
    try {
      const refreshed = await invoke<NetworkOverview | null>("refresh_campus_network");
      // A probe result is delivered asynchronously through the event channel. Do not let the
      // command's cached snapshot overwrite a newer event that arrived while it was queued.
      if (mounted.current && overviewRevision === revisions.current.overview && refreshed) {
        applyOverview(refreshed);
      }
    } catch (cause) {
      if (showError && mounted.current) setError(String(cause));
    } finally {
      refreshBusyRef.current = false;
      if (mounted.current) setRefreshBusy(false);
    }
  };

  const refresh = async () => {
    const request = ++revisions.current.refresh;
    const before = { ...revisions.current };
    const [nextOverview, nextSettings, nextStatus, nextStaffSettings, nextStaffStatus] =
      await Promise.all([
        invoke<NetworkOverview>("get_network_overview"),
        invoke<Settings>("get_dorm_network_settings"),
        invoke<Status>("get_dorm_login_status"),
        invoke<StaffSettings>("get_staff_network_settings"),
        invoke<StaffStatus>("get_staff_login_status"),
      ]);
    if (!mounted.current || request !== revisions.current.refresh) return;
    if (before.overview === revisions.current.overview) applyOverview(nextOverview);
    if (before.settings === revisions.current.settings) applySettings(nextSettings);
    if (before.status === revisions.current.status) setStatus(nextStatus);
    if (before.staffSettings === revisions.current.staffSettings)
      setStaffSettings(nextStaffSettings ?? initialStaffSettings);
    if (before.staffStatus === revisions.current.staffStatus)
      setStaffStatus(nextStaffStatus ?? initialStaffStatus);
  };

  // 初始加载与「重试」共用：成功即进入就绪并清除页面级错误，失败进入可重试的错误态。
  const loadAll = async () => {
    setLoad({ status: "loading" });
    try {
      await refresh();
      if (mounted.current) {
        setLoad({ status: "ready" });
        setError(undefined);
      }
    } catch (cause) {
      if (mounted.current) setLoad({ status: "error", message: String(cause) });
    }
  };

  useEffect(() => {
    mounted.current = true;
    let active = true;
    const disposers: (() => void)[] = [];
    const subscribe = async <T,>(event: string, callback: (payload: T) => void) => {
      try {
        const dispose = await listen<T>(event, ({ payload }) => {
          if (active) callback(payload);
        });
        if (active) disposers.push(dispose);
        else dispose();
      } catch (cause) {
        if (active) setError(String(cause));
      }
    };

    // Load cached settings/status immediately; this read-only batch does not
    // publish a probe result and must not wait for listener registration.
    void loadAll();

    // Install all listeners before starting a probe. The first snapshot can be
    // published immediately, so starting the monitor before `listen` resolves
    // would otherwise lose the only fast connection event.
    void (async () => {
      await Promise.all([
        subscribe<NetworkOverview>("network-overview-changed", (next) => {
          revisions.current.overview++;
          applyOverview(next);
        }),
        subscribe<Settings>("dorm-network-settings-changed", (next) => {
          revisions.current.settings++;
          applySettings(next);
        }),
        subscribe<Status>("dorm-login-status-changed", (next) => {
          revisions.current.status++;
          setStatus(next);
        }),
        subscribe<StaffSettings>("staff-network-settings-changed", (next) => {
          revisions.current.staffSettings++;
          setStaffSettings(next);
        }),
        subscribe<StaffStatus>("staff-login-status-changed", (next) => {
          revisions.current.staffStatus++;
          setStaffStatus(next);
        }),
      ]);
      if (!active) return;
      void invoke("request_campus_network_permissions").catch(() => {
        // Permission denial or unsupported platforms must not block network status refresh.
      });
      // Queue the portal probe independently from the initial metadata load. The command only
      // enqueues background work, so a slow portal must never delay the rest of the page.
      void refreshConnection(false);
    })();

    return () => {
      active = false;
      mounted.current = false;
      for (const dispose of disposers) dispose();
    };
  }, []);

  const saveStaff = async (input: StaffSettingsInput) => {
    const revision = revisions.current.staffSettings;
    const next = await invoke<StaffSettings>("save_staff_network_settings", { input });
    if (mounted.current && revision === revisions.current.staffSettings) setStaffSettings(next);
    return next;
  };
  const applyStaff = async (confirmationToken?: string) => {
    try {
      return await invoke<StaffApplyResult>("apply_staff_network_mode", {
        confirmationToken: confirmationToken ?? null,
      });
    } finally {
      await refresh().catch((cause) => {
        if (mounted.current) toast.danger("校园网状态刷新失败", { description: String(cause) });
      });
    }
  };

  const save = async () => {
    if (dormPending) return;
    const settingsRevision = revisions.current.settings;
    setDormPending("save");
    try {
      const next = await invoke<Settings>("save_dorm_network_settings", {
        input: {
          enabled,
          credentialMode: mode,
          campusCardNumber: mode === "custom" ? card || undefined : undefined,
          password: mode === "custom" ? password || undefined : undefined,
        },
      });
      if (settingsRevision === revisions.current.settings) applySettings(next);
      setPassword("");
      toast.success("宿舍区设置已保存");
    } catch (cause) {
      toast.danger("保存宿舍区设置失败", { description: String(cause) });
    } finally {
      setDormPending(null);
    }
  };

  const manualLogin = async () => {
    if (dormPending) return;
    setDormPending("login");
    try {
      await invoke("trigger_dorm_login");
    } catch (cause) {
      toast.danger("宿舍区登录失败", { description: String(cause) });
    } finally {
      setDormPending(null);
    }
  };

  // 断开：确认/失败弹窗 → 执行中（不可关闭）→ 成功自动关闭 + toast；失败保留弹窗。
  const runDisconnect = async () => {
    if (disconnect?.step === "running") return;
    setDisconnect({ step: "running" });
    const outcome = await disconnectPending.run("disconnect", () =>
      invoke<LogoutResult>("logout_campus_network"),
    );
    if (!outcome.ok) {
      if (outcome.skipped) return;
      setDisconnect({
        step: "failed",
        title: "断开校园网失败",
        message: errorMessage(outcome.error, "断开失败，请重试。"),
      });
      void refresh().catch(() => {});
      return;
    }
    const result = outcome.value;
    if (result.outcome === "disconnected") {
      setDisconnect(null);
      if (result.message) toast.success("已断开校园网", { description: result.message });
      else toast.success("已断开校园网");
    } else if (result.outcome === "alreadyOffline") {
      setDisconnect(null);
      if (result.message) toast.success("当前已离线", { description: result.message });
      else toast.success("当前已离线");
    } else {
      setDisconnect({
        step: "failed",
        title: result.outcome === "failed" ? "断开未完成" : "断开结果待确认",
        message: result.message || "请稍后检查网络状态。",
      });
      void refresh().catch(() => {});
      return;
    }
    void refresh().catch((cause) => {
      toast.danger("校园网状态刷新失败", { description: String(cause) });
    });
  };

  const modeDescription = useMemo(
    () =>
      mode === "currentAccount"
        ? "复用当前已登录用户的校园卡号和密码。"
        : "使用单独保存的宿舍区校园网凭据。密码只写入系统凭据库。",
    [mode],
  );

  // 状态卡跟随实际探测到的区域；上方区域 tab 只用于选择要配置的面板。
  const statusView =
    overview.region === "staff" ? staffStatus : overview.region === "dormitory" ? status : null;

  return (
    <PageShell>
      <div className="campus-network-layout">
        <ModuleHeader
          title="校园网登录"
          description={`当前推测网络状态：${statusText(overview)}`}
        />

        {load.status === "loading" && <LoadingState message="正在读取校园网配置…" />}
        {load.status === "error" && (
          <EmptyState
            variant="error"
            title="校园网配置加载失败"
            description={load.message}
            onRetry={() => void loadAll()}
          />
        )}
        {load.status === "ready" && (
          <>
            <AdaptiveTabs
              className="campus-region-tabs"
              variant="standard"
              label="校园网区域"
              ariaLabel="校园网区域"
              value={regionTab}
              onChange={(value) => {
                initializedTab.current = true;
                setRegionTab(value as RegionTab);
              }}
              items={[
                { value: "staff", label: "教工区" },
                { value: "dormitory", label: "宿舍区" },
              ]}
            />
            {/* 两个区域面板常驻挂载，仅切换可见性：切 tab 不会打断进行中的
                教工区 apply、确认弹窗或未保存草稿。`aria-hidden`/`inert` 与
                `hidden` 一起把非活动面板移出可访问树，避免查询与焦点命中。 */}
            <div
              data-region-panel="staff"
              hidden={regionTab !== "staff"}
              aria-hidden={regionTab !== "staff"}
              inert={regionTab !== "staff"}
            >
              <StaffNetworkPanel
                settings={staffSettings}
                status={staffStatus}
                disabled={operationBusy}
                onSave={saveStaff}
                onApply={applyStaff}
              />
            </div>
            <div
              data-region-panel="dormitory"
              hidden={regionTab !== "dormitory"}
              aria-hidden={regionTab !== "dormitory"}
              inert={regionTab !== "dormitory"}
            >
              <Card className="network-zone" variant="outlined">
                <div slot="headline">宿舍区</div>
                <div slot="subhead">门户地址：172.30.255.42</div>
                <div className="p-4">
                  <div className="network-setting-row">
                    <Switch
                      checked={enabled}
                      disabled={operationBusy}
                      onChange={(e: any) => setEnabled(e.detail?.checked ?? e.target?.checked)}
                      aria-label="开启宿舍区自动登录"
                    >
                      <div className="flex flex-col gap-0.5 ml-2">
                        <strong>自动登录</strong>
                        <span>检测到宿舍区网络后自动认证</span>
                      </div>
                    </Switch>
                  </div>
                  <AdaptiveTabs
                    className="network-credential-tabs"
                    variant="segmented"
                    label="宿舍区凭据来源"
                    injectDrawer={false}
                    value={mode}
                    onChange={(value) => {
                      if (!operationBusy) setMode(value as CredentialMode);
                    }}
                    items={[
                      {
                        value: "currentAccount",
                        label: "复用已登录账号",
                        disabled: operationBusy,
                      },
                      { value: "custom", label: "单独配置", disabled: operationBusy },
                    ]}
                  />
                  <p className="field-hint">{modeDescription}</p>
                  {mode === "custom" && (
                    <div className="custom-network-form">
                      <TextField
                        label="校园卡号"
                        disabled={operationBusy}
                        className="field"
                        name="dormCampusCard"
                        value={card}
                        {...bindField(setCard)}
                        placeholder="6 位数字"
                      />
                      <TextField
                        label="密码"
                        type="password"
                        disabled={operationBusy}
                        className="field"
                        name="dormPassword"
                        value={password}
                        {...bindField(setPassword)}
                        placeholder={
                          settings?.customConfigured ? "已保存（输入新密码可覆盖）" : "请输入密码"
                        }
                      />
                    </div>
                  )}
                  <div className="network-action-row">
                    <Button variant="filled" disabled={operationBusy} onClick={() => void save()}>
                      {dormPending === "save" && <LoadingIndicator size={16} />}保存设置
                    </Button>
                    <Button
                      variant="outlined"
                      disabled={operationBusy || loginInProgress || overview.region === "staff"}
                      onClick={() => void manualLogin()}
                    >
                      {dormPending === "login" && <LoadingIndicator size={16} />}立即登录
                    </Button>
                  </div>
                </div>
              </Card>
            </div>
          </>
        )}

        {load.status === "ready" && (
          <Card className="network-status-card" variant="outlined">
            <div slot="headline">当前状态（{regionLabel(overview.region)}）</div>
            <div slot="subhead">
              {disconnect?.step === "running"
                ? "正在断开连接…"
                : statusView
                  ? loginStatusText[statusView.state]
                  : "未识别到校园网门户"}
            </div>
            <div className="network-action-row" slot="actions">
              <Button
                variant="outlined"
                disabled={operationBusy || refreshBusy}
                onClick={() => void refreshConnection()}
              >
                {refreshBusy && <LoadingIndicator size={16} />}
                刷新连接状态
              </Button>
              <Button
                variant="filled"
                disabled={
                  operationBusy ||
                  (overview.region === "none" &&
                    !settings?.enabled &&
                    !staffSettings.enabled &&
                    !loginInProgress)
                }
                onClick={() => setDisconnect({ step: "confirm" })}
              >
                {disconnect?.step === "running" && <LoadingIndicator size={16} />}
                断开连接
              </Button>
            </div>
            <div className="network-status-grid p-4">
              <span>SSID</span>
              <strong>
                {overview.ssid === "有线连接" ||
                (overview.region === "dormitory" && status.ssid === "有线连接")
                  ? "有线连接"
                  : overview.ssid ||
                    (overview.region === "dormitory" ? status.ssid : undefined) ||
                    "未读取"}
              </strong>
              <span>网络连接</span>
              <strong>
                {overview.connected === false
                  ? "无网络"
                  : overview.connected === true
                    ? "已连接网络"
                    : "未知"}
              </strong>
              <span>宿舍区门户</span>
              <strong>{overview.portalReachable ? "可达" : "不可达"}</strong>
              <span>教工区门户</span>
              <strong>{overview.staffHostReachable ? "可达" : "不可达"}</strong>
              {statusView && (
                <>
                  <span>连续失败</span>
                  <strong>{statusView.consecutiveFailures} 次</strong>
                  <span>下次探测</span>
                  <strong>{formatTime(statusView.nextProbeAtMs)}</strong>
                  <span>最近结果</span>
                  <strong>
                    {(overview.region === "staff"
                      ? staffStatus.message
                      : status.lastResult?.message) || "暂无"}
                  </strong>
                </>
              )}
              {overview.state === "connected" && overview.region === "dormitory" && (
                <>
                  <span className="network-session-heading">宿舍区在线详情</span>
                  <span>校园卡号</span>
                  <strong>{overview.portalSession?.campusCardNumber || "网关未提供"}</strong>
                  <span>用户姓名</span>
                  <strong>{overview.portalSession?.onlineUsername || "网关未提供"}</strong>
                  <span>登录 IP</span>
                  <strong>{overview.portalSession?.loginIp || "网关未提供"}</strong>
                  <span>登录开始</span>
                  <strong>{overview.portalSession?.loginStartedAt || "网关未提供"}</strong>
                  <span>登录结束</span>
                  <strong>{overview.portalSession?.loginExpiresAt || "网关未提供"}</strong>
                  <span>已用时长</span>
                  <strong>{formatPortalDuration(overview.portalSession?.onlineDuration)}</strong>
                  <span>已用流量</span>
                  <strong>{formatPortalFlow(overview.portalSession?.onlineFlow)}</strong>
                </>
              )}
              {overview.state === "connected" && overview.region === "staff" && (
                <>
                  <span className="network-session-heading">教工区在线详情</span>
                  <span>登录账号</span>
                  <strong>{overview.staffSession?.username || "网关未提供"}</strong>
                  <span>姓名</span>
                  <strong>{overview.staffSession?.realName || "网关未提供"}</strong>
                  <span>IPv4</span>
                  <strong>{overview.staffSession?.loginIp || "网关未提供"}</strong>
                  <span>IPv6</span>
                  <strong>{overview.staffSession?.loginIpv6 || "网关未提供"}</strong>
                  <span>MAC 地址</span>
                  <strong>{overview.staffSession?.userMac || "网关未提供"}</strong>
                  <span>登录开始</span>
                  <strong>{formatLoginStartedAt(overview.staffSession?.loginStartedAtMs)}</strong>
                  <span>累计已用流量</span>
                  <strong>{formatUsedBytes(overview.staffSession?.usedBytes)}</strong>
                  <span>累计已用时长</span>
                  <strong>{formatUsedSeconds(overview.staffSession?.usedSeconds)}</strong>
                </>
              )}
            </div>
          </Card>
        )}
        {load.status === "ready" && error && (
          <Banner status="danger">
            <Banner.Title>校园网状态获取失败</Banner.Title>
            <Banner.Description>{error}</Banner.Description>
          </Banner>
        )}
        {load.status === "ready" &&
          disconnect !== null &&
          disconnect.step !== "running" &&
          createPortal(
            <Dialog
              open
              headline={disconnect.step === "confirm" ? "确认断开校园网？" : disconnect.title}
              onClose={() =>
                setDisconnect((current) => (current?.step === disconnect.step ? null : current))
              }
            >
              {disconnect.step === "confirm" ? (
                <div>
                  {overview.region === "dormitory"
                    ? "宿舍区会注销当前设备并解绑本机 MAC，同时关闭自动连接。不会退出应用账号、删除已保存的凭据，也不会注销其他设备。"
                    : "将注销当前设备的校园网会话，并关闭自动连接。不会退出应用账号、删除已保存的凭据，也不会注销其他设备。"}
                </div>
              ) : (
                <>
                  <Banner status="danger">
                    <Banner.Description>{disconnect.message}</Banner.Description>
                  </Banner>
                  <p className="field-hint">可重试，或关闭弹窗后检查网络状态。</p>
                </>
              )}
              <div slot="actions">
                {disconnect.step === "confirm" ? (
                  <>
                    <Button variant="text" aria-label="取消" onClick={() => setDisconnect(null)}>
                      取消
                    </Button>
                    <Button
                      variant="filled"
                      disabled={disconnectPending.isPending("disconnect")}
                      onClick={() => void runDisconnect()}
                    >
                      {disconnectPending.isPending("disconnect") && <LoadingIndicator size={16} />}
                      确认断开
                    </Button>
                  </>
                ) : (
                  <>
                    <Button variant="text" aria-label="关闭" onClick={() => setDisconnect(null)}>
                      关闭
                    </Button>
                    <Button variant="filled" onClick={() => void runDisconnect()}>
                      重试
                    </Button>
                  </>
                )}
              </div>
            </Dialog>,
            document.body,
          )}
        {load.status === "ready" &&
          disconnect?.step === "running" &&
          createPortal(
            <Dialog open noCloseOnScrimClick noCloseOnEscape headline="正在断开校园网…">
              <div className="flex items-center gap-3">
                <LoadingIndicator size={24} />
                <span>正在注销当前设备并确认下线状态，通常需要数秒…</span>
              </div>
              <div slot="actions">
                <Button variant="filled" disabled>
                  <LoadingIndicator size={16} />
                  正在断开…
                </Button>
              </div>
            </Dialog>,
            document.body,
          )}
      </div>
    </PageShell>
  );
}
