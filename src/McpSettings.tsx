import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, Switch, Button } from "material/react";
import { Banner, Chip, toast } from "./ui";
import {
  LuBot,
  LuCheck,
  LuCopy,
  LuFileCode,
  LuNetwork,
  LuRefreshCw,
  LuServer,
  LuShieldCheck,
} from "react-icons/lu";
import { copyText } from "./clipboard";

export interface McpStatusView {
  running: boolean;
  enabled: boolean;
  port: number | null;
  endpoint: string | null;
}

export default function McpSettings() {
  const [status, setStatus] = useState<McpStatusView | null>(null);
  const [loading, setLoading] = useState(true);
  const [toggling, setToggling] = useState(false);
  const [copiedUrl, setCopiedUrl] = useState(false);
  const [copiedPrompt, setCopiedPrompt] = useState(false);
  const [copiedJson, setCopiedJson] = useState(false);

  const fetchStatus = async () => {
    setLoading(true);
    try {
      const res = await invoke<McpStatusView>("get_mcp_server_status");
      setStatus(res);
    } catch {
      setStatus({ running: false, enabled: false, port: null, endpoint: null });
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (enabled: boolean) => {
    setToggling(true);
    try {
      const res = await invoke<McpStatusView>("set_mcp_server_enabled", { enabled });
      setStatus(res);
      toast.success(enabled ? "MCP 本地服务已开启并监听端口" : "MCP 本地服务已停止，端口已关闭");
    } catch (err) {
      toast.danger("切换 MCP 服务状态失败", { description: String(err) });
    } finally {
      setToggling(false);
    }
  };

  useEffect(() => {
    void fetchStatus();
  }, []);

  const endpoint = status?.endpoint ?? `http://127.0.0.1:${status?.port ?? 53901}/mcp`;

  const agentPrompt = `请帮我连接并配置泥深（your-szu-life）的本地公开 MCP 服务：
- 服务端点 URL: ${endpoint}
- 传输协议: Streamable HTTP (SSE)
- 服务名称: your-szu-life
- 监听范围: 仅限本机本地回环 127.0.0.1，高位独立端口（支持桌面与安卓端）
- 支持的功能能力:
  * 统一认证状态与个人学工档案 (auth_*)
  * 校园网络状态、宿舍/办公区登录与网络诊断 (network_*)
  * 宿舍电费实时余额、度数与历史充值记录 (electricity_*)
  * 选课整体快照、已选课表、全校搜课与教务绩点GPA (enrollment_*)
  * UOOC慕课在修进度、日常签到、考试作业与题库RAG答题 (uooc_*)
  * SZU OJ 在线评测比赛/题目、代码提交与实时判题结果 (oj_*)
  * 本地离线 OCR 验证码识别与 BGE 语义向量提取 (ocr_*, embedding_*)
  * 客户端页面直达与窗口激活 (open_page，URI 规范: your-szu-life://page/...)
  * 实时数据资源快照订阅 (your-szu-life://data/...)

请在当前环境的 MCP 配置文件（如 mcpServers 配置）中为我注册此服务。`;

  const jsonConfig = JSON.stringify(
    {
      mcpServers: {
        "your-szu-life": {
          url: endpoint,
        },
      },
    },
    null,
    2,
  );

  const handleCopy = async (text: string, type: "url" | "prompt" | "json") => {
    const ok = await copyText(text);
    if (ok) {
      if (type === "url") {
        setCopiedUrl(true);
        setTimeout(() => setCopiedUrl(false), 2000);
        toast.success("MCP 服务端点 URL 已复制到剪贴板");
      } else if (type === "prompt") {
        setCopiedPrompt(true);
        setTimeout(() => setCopiedPrompt(false), 2000);
        toast.success("指导 Agent 安装的提示词已复制到剪贴板");
      } else {
        setCopiedJson(true);
        setTimeout(() => setCopiedJson(false), 2000);
        toast.success("mcpServers JSON 配置已复制到剪贴板");
      }
    } else {
      toast.danger("复制失败，请手动选择复制");
    }
  };

  return (
    <div className="grid gap-6 w-full min-w-0 max-w-full">
      {/* 状态与配置信息卡片 */}
      <Card
        aria-labelledby="mcp-status-title"
        variant="outlined"
        className="w-full min-w-0 max-w-full"
      >
        <div
          slot="headline"
          className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 min-w-0 w-full"
        >
          <div
            id="mcp-status-title"
            className="flex flex-wrap items-center gap-2 min-w-0 font-semibold text-base"
          >
            <LuServer aria-hidden="true" className="size-5 text-primary shrink-0" />
            <span>本地 MCP 服务状态</span>
            {loading || toggling ? (
              <LoadingIndicator size={16} />
            ) : status?.running ? (
              <Chip size="sm" variant="soft" color="success">
                端口已开启 ({status?.port ?? 53901})
              </Chip>
            ) : (
              <Chip size="sm" variant="soft" color="danger">
                已关闭
              </Chip>
            )}
          </div>
          <Button
            size="xs"
            variant="outlined"
            disabled={loading || toggling}
            onClick={() => void fetchStatus()}
            className="flex items-center gap-1.5 shrink-0 self-start sm:self-auto"
          >
            <LuRefreshCw
              aria-hidden="true"
              className={`size-3.5 ${loading ? "animate-spin" : ""}`}
            />
            刷新状态
          </Button>
        </div>
        <div slot="subhead" className="mt-1.5 text-xs text-muted leading-relaxed">
          your-szu-life 提供本地公开的 Model Context Protocol
          服务，支持外部大模型客户端与本地智能体直接联动。
        </div>

        <div className="grid gap-4 w-full min-w-0 max-w-full p-4">
          {/* 开关直接管理端口 */}
          <div className="rounded-lg border border-border/50 bg-surface/50 p-3.5 w-full min-w-0 max-w-full">
            <Switch
              checked={status?.running ?? false}
              disabled={loading || toggling}
              onChange={(e: any) => void handleToggle(e.detail?.checked ?? e.target?.checked)}
              aria-label="管理本地 MCP 服务端口开启状态"
              className="w-full max-w-full"
            >
              <div className="flex flex-col gap-0.5 min-w-0 flex-1 ml-2">
                <div className="font-medium text-sm text-foreground">开启 MCP 服务端口</div>
                <div className="text-xs text-muted font-normal leading-relaxed">
                  开启后将在高位独立端口监听 Localhost 请求；关闭后立即停止服务并彻底释放关闭端口。
                </div>
              </div>
            </Switch>
          </div>

          {status?.running ? (
            <div className="grid gap-3 rounded-lg border border-border/50 bg-surface/50 p-3.5 w-full min-w-0 max-w-full overflow-hidden">
              <div className="flex flex-wrap items-center justify-between gap-2 min-w-0">
                <span className="text-xs font-medium text-muted">
                  服务端点 URL (Streamable HTTP / SSE)
                </span>
                <Button
                  size="xs"
                  variant="text"
                  onClick={() => void handleCopy(endpoint, "url")}
                  className="h-7 gap-1 px-2 text-xs shrink-0"
                >
                  {copiedUrl ? (
                    <LuCheck aria-hidden="true" className="size-3.5 text-success" />
                  ) : (
                    <LuCopy aria-hidden="true" className="size-3.5" />
                  )}
                  <span>{copiedUrl ? "已复制" : "复制 URL"}</span>
                </Button>
              </div>
              <code className="block select-all break-all rounded bg-background px-3 py-2 font-mono text-xs sm:text-sm text-foreground border border-border max-w-full overflow-x-auto">
                {endpoint}
              </code>
              <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted">
                <span className="flex items-center gap-1">
                  <LuShieldCheck aria-hidden="true" className="size-3.5 text-success shrink-0" />
                  仅监听回环地址 127.0.0.1
                </span>
                <span className="flex items-center gap-1">
                  <LuNetwork aria-hidden="true" className="size-3.5 shrink-0" />
                  独立高位端口: {status?.port ?? 53901}
                </span>
              </div>
            </div>
          ) : (
            <Banner status="warning" className="w-full min-w-0 max-w-full">
              <Banner.Title className="text-sm font-medium">服务端口已关闭</Banner.Title>
              <Banner.Description className="text-xs leading-relaxed text-muted">
                本地 MCP 端口已彻底释放关闭，外部客户端将无法连接。点击上方开关可随时开启。
              </Banner.Description>
            </Banner>
          )}

          <Banner status="accent" className="w-full min-w-0 max-w-full">
            <Banner.Title className="text-sm font-medium">安全与会话说明</Banner.Title>
            <Banner.Description className="text-xs leading-relaxed text-muted">
              本地公开 MCP 服务直接共享 your-szu-life
              内存中的活动会话与凭据（统一认证登录态、网络连接、选课上下文）。由于服务仅监听本机
              Localhost（桌面端及安卓端回环地址），外部网络无法访问，无需在外部重复输入账号密码。
            </Banner.Description>
          </Banner>
        </div>
      </Card>

      {/* 指导 Agent 安装的 Prompt 卡片 */}
      <Card
        aria-labelledby="mcp-prompt-title"
        variant="outlined"
        className="w-full min-w-0 max-w-full"
      >
        <div
          slot="headline"
          className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 min-w-0 w-full"
        >
          <div
            id="mcp-prompt-title"
            className="flex flex-wrap items-center gap-2 min-w-0 font-semibold text-base"
          >
            <LuBot aria-hidden="true" className="size-5 text-primary shrink-0" />
            <span>指导 AI Agent 安装该 MCP 服务</span>
          </div>
          <Button
            size="xs"
            variant="filled"
            onClick={() => void handleCopy(agentPrompt, "prompt")}
            className="gap-1.5 shrink-0 self-start sm:self-auto"
          >
            {copiedPrompt ? (
              <LuCheck aria-hidden="true" className="size-4 text-success" />
            ) : (
              <LuCopy aria-hidden="true" className="size-4" />
            )}
            <span>{copiedPrompt ? "已复制提示词" : "复制安装提示词"}</span>
          </Button>
        </div>
        <div slot="subhead" className="mt-1.5 text-xs text-muted leading-relaxed">
          复制下方提示词直接发送给 Claude Code、Cursor、Windsurf 或其他 AI 编程助手，即可一键完成
          MCP 服务的自动识别与配置。
        </div>

        <div className="w-full min-w-0 max-w-full p-4 overflow-hidden">
          <pre className="max-h-64 overflow-x-auto overflow-y-auto whitespace-pre-wrap break-all rounded-lg border border-border bg-background p-3.5 font-mono text-xs text-foreground leading-relaxed max-w-full">
            {agentPrompt}
          </pre>
        </div>
      </Card>

      {/* 客户端 JSON 配置卡片 */}
      <Card
        aria-labelledby="mcp-json-title"
        variant="outlined"
        className="w-full min-w-0 max-w-full"
      >
        <div
          slot="headline"
          className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 min-w-0 w-full"
        >
          <div
            id="mcp-json-title"
            className="flex flex-wrap items-center gap-2 min-w-0 font-semibold text-base"
          >
            <LuFileCode aria-hidden="true" className="size-5 text-secondary shrink-0" />
            <span className="break-all">配置文件片段 (Claude Desktop / Cursor)</span>
          </div>
          <Button
            size="xs"
            variant="outlined"
            onClick={() => void handleCopy(jsonConfig, "json")}
            className="gap-1.5 shrink-0 self-start sm:self-auto"
          >
            {copiedJson ? (
              <LuCheck aria-hidden="true" className="size-3.5 text-success" />
            ) : (
              <LuCopy aria-hidden="true" className="size-3.5" />
            )}
            <span>{copiedJson ? "已复制配置" : "复制 JSON"}</span>
          </Button>
        </div>
        <div slot="subhead" className="mt-1.5 text-xs text-muted leading-relaxed">
          手动配置到 Claude Desktop 的 <code>claude_desktop_config.json</code> 或 Cursor 的 MCP
          设置中。
        </div>

        <div className="w-full min-w-0 max-w-full p-4 overflow-hidden">
          <pre className="overflow-x-auto max-w-full rounded-lg border border-border bg-background p-3.5 font-mono text-xs text-foreground">
            {jsonConfig}
          </pre>
        </div>
      </Card>
    </div>
  );
}
