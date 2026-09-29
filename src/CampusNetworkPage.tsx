import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Alert,
  AlertDialog,
  Button,
  Card,
  CloseButton,
  FieldError,
  Input,
  Label,
  Spinner,
  Switch,
  Tabs,
  TextField,
} from "@heroui/react";
import { LuArrowLeft } from "react-icons/lu";
import { PageShell } from "./components";
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
type Settings = { enabled: boolean; credentialMode: CredentialMode; customConfigured: boolean };
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
type Log = {
  timestampMs: number;
  stage: string;
  outcome: string;
  portalReachable: boolean;
  message?: string | null;
  result?: number | null;
  retCode?: number | null;
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

function statusText(overview: NetworkOverview) {
  if (overview.state === "noNetwork") return "无网络";
  const region =
    overview.region === "staff"
      ? "教工区"
      : overview.region === "dormitory"
        ? "宿舍区"
        : "未识别区域";
  if (overview.state === "unknown") {
    return overview.connected === true ? "已连接(" + region + ")" : "状态待确认(" + region + ")";
  }
  return (overview.state === "connected" ? "已连接(" : "未登录(") + region + ")";
}

function formatTime(value?: number | null) {
  if (!value) return "暂未安排";
  return new Date(value).toLocaleTimeString();
}

function formatGatewayValue(value?: number | null) {
  if (value == null) return "网关未提供";
  if (value === 4294967295) return "未知/不限";
  return String(value);
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
  const navigation = useLogicalNavigation();
  const [overview, setOverview] = useState<NetworkOverview>(initialOverview);
  const [settings, setSettings] = useState<Settings>();
  const [status, setStatus] = useState<Status>(initialStatus);
  const [logs, setLogs] = useState<Log[]>([]);
  const [staffSettings, setStaffSettings] = useState<StaffSettings>(initialStaffSettings);
  const [staffStatus, setStaffStatus] = useState<StaffStatus>(initialStaffStatus);
  const [regionTab, setRegionTab] = useState<RegionTab>("dormitory");
  const [mode, setMode] = useState<CredentialMode>("currentAccount");
  const [enabled, setEnabled] = useState(false);
  const [card, setCard] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const initializedTab = useRef(false);
  const mounted = useRef(false);
  const refreshBusyRef = useRef(false);
  const revisions = useRef({
    overview: 0,
    settings: 0,
    status: 0,
    logs: 0,
    staffSettings: 0,
    staffStatus: 0,
    refresh: 0,
  });
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [disconnectBusy, setDisconnectBusy] = useState(false);
  const [refreshBusy, setRefreshBusy] = useState(false);
  const [logoutResult, setLogoutResult] = useState<LogoutResult>();
  const operationBusy = busy || disconnectBusy;
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
  };

  const refreshConnection = async (showError = true) => {
    if (refreshBusyRef.current) return;
    refreshBusyRef.current = true;
    setRefreshBusy(true);
    if (showError) setError("");
    const overviewRevision = revisions.current.overview;
    try {
      const refreshed = await invoke<NetworkOverview | null>("refresh_campus_network");
      // A probe result is delivered asynchronously through the event channel. Do not let the
      // command's cached snapshot overwrite a newer event that arrived while it was queued.
      if (mounted.current && overviewRevision === revisions.current.overview && refreshed) {
        applyOverview(refreshed);
      }
      return refreshed;
    } catch (cause) {
      if (showError && mounted.current) setError(String(cause));
      return null;
    } finally {
      refreshBusyRef.current = false;
      if (mounted.current) setRefreshBusy(false);
    }
  };

  const refresh = async () => {
    const request = ++revisions.current.refresh;
    const before = { ...revisions.current };
    const [nextOverview, nextSettings, nextStatus, nextLogs, nextStaffSettings, nextStaffStatus] =
      await Promise.all([
        invoke<NetworkOverview>("get_network_overview"),
        invoke<Settings>("get_dorm_network_settings"),
        invoke<Status>("get_dorm_login_status"),
        invoke<Log[]>("get_dorm_login_logs"),
        invoke<StaffSettings>("get_staff_network_settings"),
        invoke<StaffStatus>("get_staff_login_status"),
      ]);
    if (!mounted.current || request !== revisions.current.refresh) return;
    if (before.overview === revisions.current.overview) applyOverview(nextOverview);
    if (before.settings === revisions.current.settings) applySettings(nextSettings);
    if (before.status === revisions.current.status) setStatus(nextStatus);
    if (before.logs === revisions.current.logs) setLogs(nextLogs);
    if (before.staffSettings === revisions.current.staffSettings)
      setStaffSettings(nextStaffSettings ?? initialStaffSettings);
    if (before.staffStatus === revisions.current.staffStatus)
      setStaffStatus(nextStaffStatus ?? initialStaffStatus);
  };

  useEffect(() => {
    mounted.current = true;
    let active = true;
    const disposers: (() => void)[] = [];
    const delayedRefreshes: number[] = [];
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
    void refresh().catch((cause) => {
      if (active) setError(String(cause));
    });

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
        subscribe<Log>("dorm-login-log-added", (next) => {
          revisions.current.logs++;
          setLogs((current) => [next, ...current].slice(0, 100));
        }),
      ]);
      if (!active) return;
      void invoke("request_campus_network_permissions").catch(() => {
        // Permission denial or unsupported platforms must not block network status refresh.
      });
      // Queue the portal probe independently from the initial metadata load. The command only
      // enqueues background work, so a slow portal must never delay the rest of the page.
      void refreshConnection(false);
      [700, 1800].forEach((delay) => {
        delayedRefreshes.push(
          window.setTimeout(() => {
            if (active) void refreshConnection(false);
          }, delay),
        );
      });
    })();

    return () => {
      active = false;
      mounted.current = false;
      delayedRefreshes.forEach(window.clearTimeout);
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
        if (mounted.current) setError(String(cause));
      });
    }
  };

  const save = async () => {
    const settingsRevision = revisions.current.settings;
    setBusy(true);
    setError("");
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
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const manualLogin = async () => {
    setBusy(true);
    setError("");
    try {
      await invoke("trigger_dorm_login");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    if (disconnectBusy) return;
    setDisconnectBusy(true);
    setError("");
    setLogoutResult(undefined);
    try {
      const result = await invoke<LogoutResult>("logout_campus_network");
      setLogoutResult(result);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLogoutOpen(false);
      try {
        await refresh();
      } catch (cause) {
        setError(String(cause));
      }
      setDisconnectBusy(false);
    }
  };

  const modeDescription = useMemo(
    () =>
      mode === "currentAccount"
        ? "复用当前已登录用户的校园卡号和密码。"
        : "使用单独保存的宿舍区校园网凭据。密码只写入系统凭据库。",
    [mode],
  );

  return (
    <PageShell>
      <div className="campus-network-layout">
        <CloseButton
          data-webview-navigation
          className="campus-network-back"
          aria-label="返回上一级"
          onPress={() => void navigation.back()}
        >
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <header className="campus-network-header">
          <div>
            <span data-webview-navigation className="eyebrow">
              NETWORK ACCESS
            </span>
            <h1 data-webview-navigation>校园网登录</h1>
            <p className="page-description">当前推测网络状态：{statusText(overview)}</p>
          </div>
        </header>

        <Tabs
          className="campus-region-tabs"
          selectedKey={regionTab}
          onSelectionChange={(key) => {
            initializedTab.current = true;
            setRegionTab(String(key) as RegionTab);
          }}
        >
          <Tabs.List aria-label="校园网区域">
            <Tabs.Tab id="staff">教工区</Tabs.Tab>
            <Tabs.Tab id="dormitory">宿舍区</Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel id="staff">
            <StaffNetworkPanel
              settings={staffSettings}
              status={staffStatus}
              disabled={operationBusy}
              onSave={saveStaff}
              onApply={applyStaff}
            />
          </Tabs.Panel>
          <Tabs.Panel id="dormitory">
            <Card className="network-zone" variant="default">
              <Card.Header>
                <Card.Title>宿舍区</Card.Title>
                <Card.Description>门户地址：172.30.255.42</Card.Description>
              </Card.Header>
              <Card.Content>
                <div className="network-setting-row">
                  <Switch
                    isSelected={enabled}
                    isDisabled={operationBusy}
                    onChange={setEnabled}
                    aria-label="开启宿舍区自动登录"
                  >
                    <Switch.Content>
                      <strong>自动登录</strong>
                      <span>检测到宿舍区网络后自动认证</span>
                    </Switch.Content>
                    <Switch.Control>
                      <Switch.Thumb />
                    </Switch.Control>
                  </Switch>
                </div>
                <Tabs
                  className="network-credential-tabs"
                  selectedKey={mode}
                  onSelectionChange={(key) => {
                    if (!operationBusy) setMode(String(key) as CredentialMode);
                  }}
                >
                  <Tabs.List aria-label="宿舍区凭据来源">
                    <Tabs.Tab id="currentAccount">复用已登录账号</Tabs.Tab>
                    <Tabs.Tab id="custom">单独配置</Tabs.Tab>
                  </Tabs.List>
                  <Tabs.Panel id="currentAccount">
                    <p className="field-hint">{modeDescription}</p>
                  </Tabs.Panel>
                  <Tabs.Panel id="custom">
                    <p className="field-hint">{modeDescription}</p>
                    <div className="custom-network-form">
                      <TextField isDisabled={operationBusy} className="field" name="dormCampusCard">
                        <Label>校园卡号</Label>
                        <Input
                          inputMode="numeric"
                          value={card}
                          onChange={(event) => setCard(event.target.value)}
                          placeholder="6 位数字"
                        />
                      </TextField>
                      <TextField isDisabled={operationBusy} className="field" name="dormPassword">
                        <Label>密码</Label>
                        <Input
                          type="password"
                          value={password}
                          onChange={(event) => setPassword(event.target.value)}
                          placeholder={
                            settings?.customConfigured ? "已保存（输入新密码可覆盖）" : "请输入密码"
                          }
                        />
                      </TextField>
                    </div>
                  </Tabs.Panel>
                </Tabs>
                {error && <FieldError className="form-error">{error}</FieldError>}
                <div className="network-action-row">
                  <Button variant="primary" isDisabled={operationBusy} onPress={() => void save()}>
                    {busy && <Spinner size="sm" color="current" />}保存设置
                  </Button>
                  <Button
                    variant="outline"
                    isDisabled={operationBusy || loginInProgress || overview.region === "staff"}
                    onPress={() => void manualLogin()}
                  >
                    立即登录
                  </Button>
                </div>
              </Card.Content>
            </Card>
          </Tabs.Panel>
        </Tabs>

        <Card className="network-status-card" variant="default">
          <Card.Header>
            <Card.Title>当前状态</Card.Title>
            <Card.Description>
              {disconnectBusy
                ? "正在断开连接…"
                : overview.region === "staff"
                  ? staffStatus.state
                  : status.state}
            </Card.Description>
            <div className="network-action-row">
              <Button
                variant="outline"
                isDisabled={operationBusy || refreshBusy}
                onPress={() => void refreshConnection()}
              >
                {refreshBusy && <Spinner size="sm" color="current" />}
                刷新连接状态
              </Button>
              <Button
                variant="danger"
                isDisabled={
                  operationBusy ||
                  (overview.region === "none" &&
                    !settings?.enabled &&
                    !staffSettings.enabled &&
                    !loginInProgress)
                }
                onPress={() => setLogoutOpen(true)}
              >
                {disconnectBusy && <Spinner size="sm" color="current" />}
                断开连接
              </Button>
            </div>
          </Card.Header>
          <Card.Content className="network-status-grid">
            <span>SSID</span>
            <strong>{overview.ssid || status.ssid || "未读取"}</strong>
            <span>网络连接</span>
            <strong>{overview.connected === false ? "无网络" : "已连接网络"}</strong>
            <span>宿舍区门户</span>
            <strong>{overview.portalReachable ? "可达" : "不可达"}</strong>
            <span>教工区门户</span>
            <strong>{overview.staffHostReachable ? "可达" : "不可达"}</strong>
            <span>连续失败</span>
            <strong>
              {overview.region === "staff"
                ? staffStatus.consecutiveFailures
                : status.consecutiveFailures}{" "}
              次
            </strong>
            <span>下次探测</span>
            <strong>
              {formatTime(
                overview.region === "staff" ? staffStatus.nextProbeAtMs : status.nextProbeAtMs,
              )}
            </strong>
            <span>最近结果</span>
            <strong>
              {(overview.region === "staff" ? staffStatus.message : status.lastResult?.message) ||
                "暂无"}
            </strong>
            {overview.state === "connected" && overview.region === "dormitory" && (
              <>
                <span className="network-session-heading">在线详情</span>
                <span className="network-session-heading" aria-hidden="true" />
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
                <span>在线时长</span>
                <strong>{formatGatewayValue(overview.portalSession?.onlineDuration)}</strong>
                <span>已用流量</span>
                <strong>{formatGatewayValue(overview.portalSession?.onlineFlow)}</strong>
              </>
            )}
            {overview.state === "connected" && overview.region === "staff" && (
              <>
                <span className="network-session-heading">教工区在线详情</span>
                <span className="network-session-heading" aria-hidden="true" />
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
          </Card.Content>
        </Card>

        <Card className="network-status-card" variant="default">
          <Card.Header>
            <Card.Title>最近记录</Card.Title>
            <Card.Description>不会记录校园卡号、密码或完整请求地址。</Card.Description>
          </Card.Header>
          <Card.Content>
            {logs.length === 0 ? (
              <p className="field-hint">暂无校园网操作记录。</p>
            ) : (
              <ul className="network-log-list">
                {logs.slice(0, 20).map((log, index) => (
                  <li key={`${log.timestampMs}-${index}`}>
                    <span>{new Date(log.timestampMs).toLocaleString()}</span>
                    <strong>{log.outcome}</strong>
                    <small>{log.message || log.stage}</small>
                  </li>
                ))}
              </ul>
            )}
          </Card.Content>
        </Card>
        <AlertDialog
          isOpen={logoutOpen}
          onOpenChange={(open) => {
            if (!disconnectBusy) setLogoutOpen(open);
          }}
        >
          <AlertDialog.Backdrop>
            <AlertDialog.Container size="sm">
              <AlertDialog.Dialog>
                <AlertDialog.Icon status="danger" />
                <AlertDialog.Header>
                  <AlertDialog.Heading>确认断开校园网？</AlertDialog.Heading>
                </AlertDialog.Header>
                <AlertDialog.Body>
                  将注销当前设备的校园网会话，并关闭自动连接。不会退出应用账号、删除已保存的凭据，也不会注销其他设备。
                </AlertDialog.Body>
                <AlertDialog.Footer>
                  <AlertDialog.CloseTrigger
                    aria-label="取消"
                    isDisabled={disconnectBusy}
                    className="dialog-cancel"
                  />
                  <Button
                    variant="danger"
                    isDisabled={disconnectBusy}
                    onPress={() => void disconnect()}
                  >
                    {disconnectBusy && <Spinner size="sm" color="current" />}
                    确认断开
                  </Button>
                </AlertDialog.Footer>
              </AlertDialog.Dialog>
            </AlertDialog.Container>
          </AlertDialog.Backdrop>
        </AlertDialog>
        {logoutResult && (
          <Alert
            status={
              logoutResult.outcome === "failed"
                ? "danger"
                : logoutResult.outcome === "unknown"
                  ? "warning"
                  : "success"
            }
          >
            <Alert.Content>
              <Alert.Title>
                {logoutResult.outcome === "unknown"
                  ? "断开结果待确认"
                  : logoutResult.outcome === "failed"
                    ? "断开未完成"
                    : "已断开校园网"}
              </Alert.Title>
              <Alert.Description>{logoutResult.message}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {error && (
          <Alert status="danger">
            <Alert.Content>
              <Alert.Title>校园网操作失败</Alert.Title>
              <Alert.Description>{error}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
      </div>
    </PageShell>
  );
}
