import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LuCheck, LuGlobe, LuRotateCcw } from "react-icons/lu";
import { Button, Card, LoadingIndicator, Radio, Radios, Switch, TextField } from "material/react";
import { Banner, SettingRow, bindField, toast } from "./ui";
import { errorMessage, useAsyncLoad } from "./async-state";
import { usePending } from "./use-pending";
import { CampusAccessSettings } from "./CampusAccessSettings";

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

/** 代理模式卡片里可编辑的六个字段；绕过开关不属于草稿，见 `saved`。 */
type Draft = {
  mode: ProxyMode;
  protocol: ProxyProtocol;
  host: string;
  username: string;
  port: string;
  password: string;
};

function draftOf(view: ProxySettingsView): Draft {
  return {
    mode: view.mode,
    protocol: view.protocol,
    host: view.host,
    username: view.username,
    port: String(view.port),
    password: "",
  };
}

const DEFAULT_DRAFT = draftOf(DEFAULT_VIEW);

const PORT_ERROR = "端口需为 1 - 65535 之间的整数。";

/** 端口必须是 1 - 65535 的整数；`Number.parseInt(port) || 0` 会把 0 也提交上去。 */
function parsePort(value: string): number | null {
  const text = value.trim();
  if (!/^\d+$/.test(text)) return null;
  const port = Number(text);
  return Number.isSafeInteger(port) && port >= 1 && port <= 65535 ? port : null;
}

/**
 * 网络设置：全局代理。
 *
 * 这里只负责收集与提交配置；「哪些请求要绕过代理」完全由后端 netproxy 模块
 * 按目标地址判断，设置页不再重复描述规则，只说明可见的结果。
 */
export function NetworkSettings() {
  const pending = usePending();
  const { state, reload } = useAsyncLoad(() => invoke<ProxySettingsView>("get_proxy_settings"));
  /** 最近一次落盘的视图：`campusThroughProxy` 与 `hasPassword` 以它为准。 */
  const [saved, setSaved] = useState<ProxySettingsView>(DEFAULT_VIEW);
  const [draft, setDraft] = useState<Draft>(DEFAULT_DRAFT);
  const [portError, setPortError] = useState<string | null>(null);

  const applyView = useCallback((view: ProxySettingsView) => {
    setSaved(view);
    setDraft(draftOf(view));
    setPortError(null);
  }, []);

  useEffect(() => {
    if (state.status === "ready") applyView(state.data);
  }, [state, applyView]);

  useEffect(() => {
    // 其它窗口改了设置时保持一致。
    let active = true;
    const subscription = listen<ProxySettingsView>("proxy-settings-changed", (event) => {
      if (active && event.payload) applyView(event.payload);
    });
    return () => {
      active = false;
      void subscription.then((unlisten) => unlisten());
    };
  }, [applyView]);

  const loading = state.status === "idle" || state.status === "loading";
  // 读取失败时同样锁住控件：此时草稿仍是默认值，保存会覆盖用户真正存下的代理配置。
  const blocked = loading || state.status === "error";

  const update = (patch: Partial<Draft>) => setDraft((current) => ({ ...current, ...patch }));

  const buildInput = (campusThroughProxyOverride?: boolean) => ({
    mode: draft.mode,
    protocol: draft.protocol,
    host: draft.host.trim(),
    // 端口已校验时用草稿值。绕过开关可以在编辑中途点下，此时草稿里可能是无效输入，
    // 沿用落盘端口而不是把 NaN（或 0）交给后端。
    port: parsePort(draft.port) ?? saved.port,
    username: draft.username.trim(),
    // 留空表示沿用已保存的密码；清空用户名会一并清掉密码。
    password: draft.password === "" ? null : draft.password,
    // 保存按钮不应改变绕过开关；使用最近一次落盘值。
    campusThroughProxy: campusThroughProxyOverride ?? saved.campusThroughProxy,
  });

  /** 校验端口；不合法时把错误挂到端口输入框上，由调用方决定是否 toast。 */
  const requirePort = (): boolean => {
    const valid = parsePort(draft.port) !== null;
    setPortError(valid ? null : PORT_ERROR);
    return valid;
  };

  const save = async () => {
    if (!requirePort()) {
      toast.danger("端口无效", { description: PORT_ERROR });
      return;
    }
    const result = await pending.run("save", () =>
      invoke<ProxySettingsView>("save_proxy_settings", { settings: buildInput() }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("保存代理设置失败", { description: errorMessage(result.error) });
      return;
    }
    applyView(result.value);
    toast.success("代理设置已保存", { description: "新的代理设置已对后续请求生效。" });
  };

  const toggleBypass = async (next: boolean) => {
    if (blocked) return;
    const previous = saved.campusThroughProxy;
    // 开关立即反映用户操作；失败时回到落盘值。
    setSaved((current) => ({ ...current, campusThroughProxy: next }));
    const result = await pending.run("bypass", () =>
      invoke<ProxySettingsView>("save_proxy_settings", { settings: buildInput(next) }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      setSaved((current) => ({ ...current, campusThroughProxy: previous }));
      toast.danger("保存绕过设置失败", { description: errorMessage(result.error) });
      return;
    }
    // 只接收服务端确认后的绕过值与密码状态；草稿里可能还有用户正在编辑的字段。
    setSaved(result.value);
    toast.success(next ? "已开启校内网代理" : "已关闭校内网代理");
  };

  const test = async () => {
    if (!requirePort()) {
      toast.danger("端口无效", { description: PORT_ERROR });
      return;
    }
    const result = await pending.run("test", () =>
      invoke<{ target: string; status: number; elapsedMs: number }>("test_proxy_settings", {
        settings: buildInput(),
        url: null,
      }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("代理不可用", { description: errorMessage(result.error) });
      return;
    }
    const { target, status, elapsedMs } = result.value;
    toast.success(`代理可用：HTTP ${status}`, {
      description: `${target}，用时 ${elapsedMs} ms`,
    });
  };

  const reset = () => {
    setDraft(DEFAULT_DRAFT);
    setPortError(null);
  };

  const manual = draft.mode === "manual";

  return (
    <>
      {loading && (
        <p className="status-line">
          <LoadingIndicator size={16} aria-label="正在读取网络设置" />
          正在读取网络设置…
        </p>
      )}

      {state.status === "error" && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>读取网络设置失败</Banner.Title>
            <Banner.Description>
              <span className="block">{state.message}</span>
              <span className="block">重试成功前不会保存任何改动。</span>
            </Banner.Description>
          </Banner>
          <Button variant="outlined" onClick={() => void reload()}>
            重试
          </Button>
        </div>
      )}

      <Card variant="outlined">
        <div slot="headline">代理模式</div>
        <div slot="subhead">对应用内所有外部网络请求生效</div>
        <div className="grid gap-3">
          <Radios
            aria-label="代理模式"
            value={draft.mode}
            disabled={blocked}
            onChange={(event) => update({ mode: event.detail.value as ProxyMode })}
          >
            {MODE_OPTIONS.map((option) => (
              <Radio key={option.value} value={option.value}>
                {option.label}
              </Radio>
            ))}
          </Radios>
          <p className="text-sm text-muted">
            {MODE_OPTIONS.find((option) => option.value === draft.mode)?.description}
          </p>
        </div>

        {manual && (
          <div className="card-group mt-4">
            <p className="card-group-title">代理服务器</p>
            {draft.protocol === "socks5" && (
              <Banner status="warning">
                <Banner.Title>应用内下载不会走 SOCKS5</Banner.Title>
                <Banner.Description>
                  aria2 不支持 SOCKS 代理，因此应用更新与本地模型下载会绕开这个代理。
                  若下载也需要代理，请改用 HTTP 代理。
                </Banner.Description>
              </Banner>
            )}
            <Radios
              aria-label="代理协议"
              value={draft.protocol}
              disabled={blocked}
              onChange={(event) => update({ protocol: event.detail.value as ProxyProtocol })}
            >
              <Radio value="http">HTTP</Radio>
              <Radio value="socks5">SOCKS5</Radio>
            </Radios>
            <div className="grid gap-4 sm:grid-cols-2">
              <TextField
                label="服务器地址"
                value={draft.host}
                disabled={blocked}
                {...bindField((value) => update({ host: value }))}
                supportingText="主机名或 IP，不要带协议前缀"
              />
              <TextField
                label="端口"
                type="number"
                value={draft.port}
                disabled={blocked}
                error={portError != null}
                {...bindField((value) => {
                  update({ port: value });
                  setPortError(null);
                })}
                supportingText={portError ?? "1 - 65535"}
              />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <TextField
                label="用户名（可选）"
                value={draft.username}
                disabled={blocked}
                {...bindField((value) => update({ username: value }))}
              />
              <TextField
                label="密码（可选）"
                type="password"
                value={draft.password}
                disabled={blocked}
                {...bindField((value) => update({ password: value }))}
                supportingText={saved.hasPassword ? "留空表示沿用已保存的密码" : undefined}
              />
            </div>
          </div>
        )}

        <div className="card-group mt-4">
          <div className="flex flex-wrap items-center gap-3">
            <Button
              variant="filled"
              disabled={blocked || pending.isPending("save")}
              onClick={() => void save()}
            >
              <LuCheck aria-hidden="true" />
              保存
            </Button>
            <Button
              variant="tonal"
              disabled={blocked || pending.isPending("test")}
              onClick={() => void test()}
            >
              测试连接
            </Button>
            <Button variant="text" disabled={blocked} onClick={reset}>
              <LuRotateCcw aria-hidden="true" />
              恢复默认
            </Button>
          </div>
        </div>
      </Card>

      <Card variant="outlined">
        <div slot="headline">绕过设置</div>
        <SettingRow
          icon={<LuGlobe aria-hidden="true" />}
          title="代理校园内网请求"
          description="白名单内的校内地址与域名，在开启后也经由代理访问。校园网登录地址与不在白名单里的内网地址、本机地址始终直连。"
        >
          <Switch
            checked={saved.campusThroughProxy}
            disabled={blocked || pending.isPending("bypass")}
            onChange={(event: any) =>
              void toggleBypass(Boolean(event.detail?.checked ?? event.target?.checked))
            }
            aria-label="校内网也走代理"
          />
        </SettingRow>
      </Card>

      <CampusAccessSettings />
    </>
  );
}
