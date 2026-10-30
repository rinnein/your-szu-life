import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, LoadingIndicator, Radio, Radios } from "material/react";
import { Banner, toast } from "../ui";
import type { StepProps } from "./step-types";

type McpStatusView = {
  running: boolean;
  enabled: boolean;
  port: number | null;
  endpoint: string | null;
};

export function McpStep({ onComplete }: StepProps) {
  const [status, setStatus] = useState<McpStatusView>();
  const [enabled, setEnabled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState<string>();

  const load = async () => {
    setLoadError(undefined);
    try {
      const value = await invoke<McpStatusView>("get_mcp_server_status");
      setStatus(value);
      setEnabled(value.running || value.enabled);
    } catch (cause) {
      setLoadError(typeof cause === "string" ? cause : "无法读取 MCP 服务状态");
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const finish = async () => {
    setBusy(true);
    try {
      const next = await invoke<McpStatusView>("set_mcp_server_enabled", { enabled });
      setStatus(next);
      toast.success(enabled ? "MCP 本地服务已开启" : "MCP 本地服务保持关闭");
      await onComplete();
    } catch (cause) {
      toast.danger("保存 MCP 服务状态失败", {
        description: typeof cause === "string" ? cause : "请重试。",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card variant="outlined">
      <div slot="headline">MCP 本地服务</div>
      <div slot="subhead">
        开启后在本机 127.0.0.1 的高位端口提供 Model Context Protocol 服务，供外部 AI
        客户端接入；不会对外网或局域网开放。
      </div>
      <div className="p-4 grid gap-4">
        {loadError && (
          <Banner status="danger">
            <Banner.Title>MCP 服务状态读取失败</Banner.Title>
            <Banner.Description>{loadError}</Banner.Description>
          </Banner>
        )}
        {!status && loadError && (
          <Button variant="outlined" onClick={() => void load()}>
            重试读取状态
          </Button>
        )}
        {!status && !loadError ? (
          <div className="flex items-center gap-2 text-sm text-muted" role="status">
            <LoadingIndicator size={16} /> 正在读取 MCP 服务状态…
          </div>
        ) : (
          <Radios
            aria-label="MCP 服务默认状态"
            value={enabled ? "on" : "off"}
            onChange={(event: any) => setEnabled(event.detail.value === "on")}
          >
            <Radio value="on">开启（推荐给 AI 客户端使用）</Radio>
            <Radio value="off">关闭</Radio>
          </Radios>
        )}
        <Banner status="accent">
          <Banner.Title>安全说明</Banner.Title>
          <Banner.Description>
            服务仅监听本机回环地址，外部网络无法访问；开启后可以随时在设置 → MCP 中关闭。
          </Banner.Description>
        </Banner>
        <Button variant="filled" disabled={busy || !status} onClick={() => void finish()}>
          {busy && <LoadingIndicator size={16} />}
          {busy ? "保存中…" : "完成"}
        </Button>
      </div>
    </Card>
  );
}
