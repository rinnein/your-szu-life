import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Alert,
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

type CredentialMode = "currentAccount" | "custom";
type RegionTab = "staff" | "dormitory";
type NetworkRegion = "none" | "staff" | "dormitory";
type NetworkState = "noNetwork" | "notLoggedIn" | "connected";
type Settings = { enabled: boolean; credentialMode: CredentialMode; customConfigured: boolean };
type PortalSession = {
  onlineUsername?: string | null;
  onlineDuration?: number | null;
  onlineFlow?: number | null;
  loginIp?: string | null;
  loginStartedAt?: string | null;
  loginExpiresAt?: string | null;
  charset?: string | null;
};
type NetworkOverview = {
  state: NetworkState;
  region: NetworkRegion;
  ssid?: string | null;
  portalReachable: boolean;
  staffHostReachable: boolean;
  portalSession?: PortalSession | null;
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
  if (overview.state === "connected" && overview.region === "dormitory") return "已连接(宿舍区)";
  if (overview.region === "staff") return "未登录(教工区)";
  return "未登录(宿舍区)";
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

function regionDefaultTab(region: NetworkRegion): RegionTab {
  return region === "staff" ? "staff" : "dormitory";
}

export default function CampusNetworkPage() {
  const navigate = useNavigate();
  const [overview, setOverview] = useState<NetworkOverview>(initialOverview);
  const [settings, setSettings] = useState<Settings>();
  const [status, setStatus] = useState<Status>(initialStatus);
  const [logs, setLogs] = useState<Log[]>([]);
  const [regionTab, setRegionTab] = useState<RegionTab>("dormitory");
  const [mode, setMode] = useState<CredentialMode>("currentAccount");
  const [enabled, setEnabled] = useState(false);
  const [card, setCard] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const initializedTab = useRef(false);

  const refresh = async () => {
    const [nextOverview, nextSettings, nextStatus, nextLogs] = await Promise.all([
      invoke<NetworkOverview>("get_network_overview"),
      invoke<Settings>("get_dorm_network_settings"),
      invoke<Status>("get_dorm_login_status"),
      invoke<Log[]>("get_dorm_login_logs"),
    ]);
    setOverview(nextOverview);
    if (!initializedTab.current) {
      setRegionTab(regionDefaultTab(nextOverview.region));
      initializedTab.current = true;
    }
    setSettings(nextSettings);
    setEnabled(nextSettings.enabled);
    setMode(nextSettings.credentialMode);
    setStatus(nextStatus);
    setLogs(nextLogs);
  };

  useEffect(() => {
    void refresh().catch((cause) => setError(String(cause)));
    let disposeOverview: (() => void) | undefined;
    let disposeStatus: (() => void) | undefined;
    let disposeLog: (() => void) | undefined;
    void listen<NetworkOverview>("network-overview-changed", (event) => {
      setOverview(event.payload);
      if (!initializedTab.current) {
        setRegionTab(regionDefaultTab(event.payload.region));
        initializedTab.current = true;
      }
    }).then((dispose) => (disposeOverview = dispose));
    void listen<Status>("dorm-login-status-changed", (event) => setStatus(event.payload)).then(
      (dispose) => (disposeStatus = dispose),
    );
    void listen<Log>("dorm-login-log-added", (event) =>
      setLogs((current) => [event.payload, ...current].slice(0, 100)),
    ).then((dispose) => (disposeLog = dispose));
    return () => {
      disposeOverview?.();
      disposeStatus?.();
      disposeLog?.();
    };
  }, []);

  const save = async () => {
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
      setSettings(next);
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
          className="campus-network-back"
          aria-label="返回上一级"
          onPress={() => void navigate({ to: "/" })}
        >
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <header className="campus-network-header">
          <div>
            <span className="eyebrow">NETWORK ACCESS</span>
            <h1>校园网登录</h1>
            <p className="page-description">当前推测网络状态：{statusText(overview)}</p>
          </div>
        </header>

        <Tabs
          className="campus-region-tabs"
          selectedKey={regionTab}
          onSelectionChange={(key) => setRegionTab(String(key) as RegionTab)}
        >
          <Tabs.List aria-label="校园网区域">
            <Tabs.Tab id="staff">教工区</Tabs.Tab>
            <Tabs.Tab id="dormitory">宿舍区</Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel id="staff">
            <Card className="network-zone network-zone-disabled" variant="default">
              <Card.Header>
                <Card.Title>教工区</Card.Title>
                <Card.Description>
                  网络状态：{overview.region === "staff" ? "已识别" : "未识别"}
                </Card.Description>
              </Card.Header>
              <Card.Content>教工区认证功能暂未支持。</Card.Content>
            </Card>
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
                  onSelectionChange={(key) => setMode(String(key) as CredentialMode)}
                >
                  <Tabs.List aria-label="宿舍区凭据来源">
                    <Tabs.Tab id="currentAccount">当前账户凭据</Tabs.Tab>
                    <Tabs.Tab id="custom">单独配置</Tabs.Tab>
                  </Tabs.List>
                  <Tabs.Panel id="currentAccount">
                    <p className="field-hint">{modeDescription}</p>
                  </Tabs.Panel>
                  <Tabs.Panel id="custom">
                    <p className="field-hint">{modeDescription}</p>
                    <div className="custom-network-form">
                      <TextField className="field" name="dormCampusCard">
                        <Label>校园卡号</Label>
                        <Input
                          inputMode="numeric"
                          value={card}
                          onChange={(event) => setCard(event.target.value)}
                          placeholder="6 位数字"
                        />
                      </TextField>
                      <TextField className="field" name="dormPassword">
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
                  <Button variant="primary" isDisabled={busy} onPress={() => void save()}>
                    {busy && <Spinner size="sm" color="current" />}保存设置
                  </Button>
                  <Button variant="outline" isDisabled={busy} onPress={() => void manualLogin()}>
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
            <Card.Description>{status.state}</Card.Description>
          </Card.Header>
          <Card.Content className="network-status-grid">
            <span>SSID</span>
            <strong>{overview.ssid || status.ssid || "未读取"}</strong>
            <span>网络连接</span>
            <strong>{overview.state === "noNetwork" ? "无网络" : "已连接网络"}</strong>
            <span>宿舍门户</span>
            <strong>{overview.portalReachable ? "可达" : "不可达"}</strong>
            <span>教工区探测</span>
            <strong>{overview.staffHostReachable ? "可达" : "不可达"}</strong>
            <span>连续失败</span>
            <strong>{status.consecutiveFailures} 次</strong>
            <span>下次探测</span>
            <strong>{formatTime(status.nextProbeAtMs)}</strong>
            <span>最近结果</span>
            <strong>{status.lastResult?.message || "暂无"}</strong>
            {overview.state === "connected" && overview.region === "dormitory" && (
              <>
                <span className="network-session-heading">在线详情</span>
                <span className="network-session-heading" aria-hidden="true" />
                <span>当前用户</span>
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
          </Card.Content>
        </Card>

        <Card className="network-status-card" variant="default">
          <Card.Header>
            <Card.Title>最近记录</Card.Title>
            <Card.Description>不会记录校园卡号、密码或完整请求地址。</Card.Description>
          </Card.Header>
          <Card.Content>
            {logs.length === 0 ? (
              <p className="field-hint">暂无登录记录。</p>
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
