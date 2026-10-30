import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LuCheck, LuGlobe, LuRotateCcw } from "react-icons/lu";
import { Button, Card, Radio, Radios, Switch, TextField } from "material/react";
import { Banner, bindField, toast } from "./ui";

/** 代理模式：关闭 / 自动（系统代理与环境变量）/ 手动。 */
export type ProxyMode = "off" | "auto" | "manual";
export type ProxyProtocol = "http" | "socks5";

/** 后端 `ProxySettingsView`：不含密码，只看是否已设置。 */
export interface ProxySettingsView {
  mode: ProxyMode;
  protocol: ProxyProtocol;
  host: string;
  port: number;
  username: string;
  hasPassword: boolean;
  campusThroughProxy: boolean;
}

const DEFAULT_VIEW: ProxySettingsView = {
  mode: "auto",
  protocol: "http",
  host: "",
  port: 8080,
  username: "",
  hasPassword: false,
  campusThroughProxy: false,
};

const MODE_OPTIONS: { value: ProxyMode; label: string; description: string }[] = [
  {
    value: "off",
    label: "关闭",
    description: "所有请求直连，连环境变量里的代理也不使用。",
  },
  {
    value: "auto",
    label: "自动",
    description: "跟随系统代理设置，读不到时回退到环境变量。",
  },
  {
    value: "manual",
    label: "手动",
    description: "使用下面填写的代理服务器。",
  },
];

/**
 * 网络设置：全局代理。
 *
 * 这里只负责收集与提交配置；「哪些请求要绕过代理」完全由后端 netproxy 模块
 * 按目标地址判断，设置页不再重复描述规则，只说明可见的结果。
 */
export function NetworkSettings() {
  const [saved, setSaved] = useState<ProxySettingsView>(DEFAULT_VIEW);
  const [mode, setMode] = useState<ProxyMode>(DEFAULT_VIEW.mode);
  const [protocol, setProtocol] = useState<ProxyProtocol>(DEFAULT_VIEW.protocol);
  const [host, setHost] = useState("");
  const [port, setPort] = useState(String(DEFAULT_VIEW.port));
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [campusThroughProxy, setCampusThroughProxy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  const applyView = (view: ProxySettingsView) => {
    setSaved(view);
    setMode(view.mode);
    setProtocol(view.protocol);
    setHost(view.host);
    setPort(String(view.port));
    setUsername(view.username);
    setPassword("");
    setCampusThroughProxy(view.campusThroughProxy);
  };

  useEffect(() => {
    let active = true;
    void invoke<ProxySettingsView>("get_proxy_settings")
      .then((view) => {
        if (active && view) applyView(view);
      })
      .catch((error) => {
        toast.danger("读取网络设置失败", { description: errorText(error) });
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    // 其它窗口改了设置时保持一致。
    const subscription = listen<ProxySettingsView>("proxy-settings-changed", (event) => {
      if (active && event.payload) applyView(event.payload);
    });
    return () => {
      active = false;
      void subscription.then((unlisten) => unlisten());
    };
  }, []);

  const buildInput = () => ({
    mode,
    protocol,
    host: host.trim(),
    port: Number.parseInt(port, 10) || 0,
    username: username.trim(),
    // 留空表示沿用已保存的密码；清空用户名会一并清掉密码。
    password: password === "" ? null : password,
    campusThroughProxy,
  });

  const save = async () => {
    setBusy(true);
    try {
      const view = await invoke<ProxySettingsView>("save_proxy_settings", {
        settings: buildInput(),
      });
      applyView(view);
      toast.success("网络设置已保存", { description: "新的代理设置已对后续请求生效。" });
    } catch (error) {
      toast.danger("保存网络设置失败", { description: errorText(error) });
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    try {
      const result = await invoke<{ target: string; status: number; elapsedMs: number }>(
        "test_proxy_settings",
        { settings: buildInput(), url: null },
      );
      toast.success(`代理可用：HTTP ${result.status}`, {
        description: `${result.target}，用时 ${result.elapsedMs} ms`,
      });
    } catch (error) {
      toast.danger("代理不可用", { description: errorText(error) });
    } finally {
      setBusy(false);
    }
  };

  const reset = () => {
    setMode(DEFAULT_VIEW.mode);
    setProtocol(DEFAULT_VIEW.protocol);
    setHost(DEFAULT_VIEW.host);
    setPort(String(DEFAULT_VIEW.port));
    setUsername(DEFAULT_VIEW.username);
    setPassword("");
    setCampusThroughProxy(DEFAULT_VIEW.campusThroughProxy);
  };

  const manual = mode === "manual";

  return (
    <div className="space-y-6 max-w-3xl">
      <Card variant="outlined">
        <div slot="headline">代理模式</div>
        <div slot="subhead">对应用内所有外部网络请求生效</div>
        <div className="space-y-3 pt-2">
          <Radios
            aria-label="代理模式"
            value={mode}
            disabled={loading}
            onChange={(event) => setMode(event.detail.value as ProxyMode)}
          >
            {MODE_OPTIONS.map((option) => (
              <Radio key={option.value} value={option.value}>
                {option.label}
              </Radio>
            ))}
          </Radios>
          <p className="text-sm text-muted-foreground">
            {MODE_OPTIONS.find((option) => option.value === mode)?.description}
          </p>
        </div>
      </Card>

      {manual && (
        <Card variant="outlined">
          <div slot="headline">代理服务器</div>
          {manual && protocol === "socks5" && (
            <Banner status="warning">
              <Banner.Title>应用内下载不会走 SOCKS5</Banner.Title>
              <Banner.Description>
                aria2 不支持 SOCKS 代理，因此应用更新与本地模型下载会绕开这个代理。
                若下载也需要代理，请改用 HTTP 代理。
              </Banner.Description>
            </Banner>
          )}
          <div className="space-y-4 pt-2">
            <Radios
              aria-label="代理协议"
              value={protocol}
              disabled={loading}
              onChange={(event) => setProtocol(event.detail.value as ProxyProtocol)}
            >
              <Radio value="http">HTTP</Radio>
              <Radio value="socks5">SOCKS5</Radio>
            </Radios>
            <div className="grid gap-4 sm:grid-cols-2">
              <TextField
                label="服务器地址"
                value={host}
                disabled={loading}
                {...bindField(setHost)}
                supportingText="主机名或 IP，不要带协议前缀"
              />
              <TextField
                label="端口"
                type="number"
                value={port}
                disabled={loading}
                {...bindField(setPort)}
                supportingText="1 - 65535"
              />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <TextField
                label="用户名（可选）"
                value={username}
                disabled={loading}
                {...bindField(setUsername)}
              />
              <TextField
                label="密码（可选）"
                type="password"
                value={password}
                disabled={loading}
                {...bindField(setPassword)}
                supportingText={saved.hasPassword ? "留空表示沿用已保存的密码" : undefined}
              />
            </div>
          </div>
        </Card>
      )}

      <Card variant="outlined">
        <div slot="headline">绕过设置</div>
        <div className="flex items-center justify-between gap-4 pt-2">
          <div className="space-y-1">
            <div className="flex items-center gap-2 font-medium text-sm text-foreground">
              <LuGlobe className="size-4 text-primary" aria-hidden="true" />
              <span>代理校园内网请求</span>
            </div>
            <p className="text-xs text-muted-foreground">
              白名单内的校内地址与域名，在开启后也经由代理访问。校园网登录地址与不在白名单里的内网地址、本机地址始终直连。
            </p>
          </div>
          <Switch
            checked={campusThroughProxy}
            disabled={loading}
            onChange={(event: any) =>
              setCampusThroughProxy(Boolean(event.detail?.checked ?? event.target?.checked))
            }
            aria-label="校内网也走代理"
          />
        </div>
      </Card>

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="filled" disabled={loading || busy} onClick={() => void save()}>
          <LuCheck aria-hidden="true" />
          保存
        </Button>
        <Button variant="tonal" disabled={loading || busy} onClick={() => void test()}>
          测试连接
        </Button>
        <Button variant="text" disabled={loading || busy} onClick={reset}>
          <LuRotateCcw aria-hidden="true" />
          恢复默认
        </Button>
      </div>
    </div>
  );
}

function errorText(error: unknown) {
  return typeof error === "string" ? error : error instanceof Error ? error.message : "未知错误";
}
