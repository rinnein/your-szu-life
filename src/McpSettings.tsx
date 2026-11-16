import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, Switch, Button } from "material/react";
import { Banner, Chip, SettingRow, toast } from "./ui";
import { errorMessage, useAsyncLoad } from "./async-state";
import { usePending } from "./use-pending";
import {
  LuBot,
  LuCopy,
  LuFileCode,
  LuNetwork,
  LuRefreshCw,
  LuServer,
  LuShieldCheck,
} from "react-icons/lu";
import { copyText } from "./clipboard";
import { useRegisterNextAction, type StepNextAction } from "./step-next-action";

export interface McpStatusView {
  running: boolean;
  enabled: boolean;
  port: number | null;
  preferredPort: number;
  endpoint: string | null;
}

/** 首选端口：后端换端口后会通过状态回报实际值，这里只用于占位展示。 */
const DEFAULT_MCP_PORT = 53901;

export default function McpSettings({
  onboarding = false,
  onComplete,
  registerNext,
}: {
  onboarding?: boolean;
  onComplete?: () => void | Promise<void>;
  registerNext?: (action: StepNextAction | null) => void;
}) {
  const {
    state: status,
    reload,
    apply,
  } = useAsyncLoad<McpStatusView>(() => invoke<McpStatusView>("get_mcp_server_status"));
  const pending = usePending();
  const toggling = pending.isPending("toggle");
  const refreshing = status.status === "loading";
  const view =
    status.status === "ready"
      ? status.data
      : status.status === "loading"
        ? (status.previous ?? null)
        : null;
  // 首次读取时还没有快照：开关先禁用，避免在未知状态下切换。
  const waiting = !view && status.status !== "error";

  const handleToggle = async (enabled: boolean) => {
    const outcome = await pending.run("toggle", () =>
      invoke<McpStatusView>("set_mcp_server_enabled", { enabled }),
    );
    if (outcome.ok) {
      apply(outcome.value);
      toast.success(enabled ? "MCP 本地服务已开启并监听端口" : "MCP 本地服务已停止，端口已关闭");
    } else if (!outcome.skipped) {
      toast.danger("切换 MCP 服务状态失败", { description: errorMessage(outcome.error) });
    }
  };

  useRegisterNextAction(
    registerNext,
    onboarding && onComplete
      ? {
          label: "下一步",
          disabled: refreshing || toggling || !view,
          busy: toggling,
          run: onComplete,
        }
      : null,
  );

  const endpoint =
    view?.endpoint ?? `http://127.0.0.1:${view?.preferredPort ?? DEFAULT_MCP_PORT}/mcp`;
  const port = view?.port ?? view?.preferredPort ?? DEFAULT_MCP_PORT;

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

  // 复制结果只走 Snackbar：按钮文案不需要在 2 秒内变一次再变回来。
  const handleCopy = async (text: string, label: string) => {
    const ok = await copyText(text);
    if (ok) toast.success(`${label}已复制到剪贴板`);
    else toast.danger("复制失败，请手动选择复制");
  };

  return (
    <>
      <Card variant="outlined">
        <div slot="headline">
          <span className="inline-flex items-center gap-2">
            <LuServer aria-hidden="true" className="size-4" />
            本地 MCP 服务状态
          </span>
        </div>
        <div slot="subhead">
          your-szu-life 提供本地公开的 Model Context Protocol
          服务，支持外部大模型客户端与本地智能体直接联动。
        </div>

        <div className="grid gap-4">
          {status.status === "error" && (
            <>
              <Banner status="danger">
                <Banner.Title>MCP 服务状态读取失败</Banner.Title>
                <Banner.Description>{status.message}</Banner.Description>
              </Banner>
              <div>
                <Button variant="outlined" onClick={() => void reload()}>
                  重试读取状态
                </Button>
              </div>
            </>
          )}

          <SettingRow
            title="开启 MCP 服务端口"
            description="开启后将在高位独立端口监听本机请求；关闭后立即停止服务并释放端口。"
          >
            <Switch
              checked={view?.running ?? false}
              disabled={refreshing || toggling || waiting}
              onChange={(e: any) =>
                void handleToggle(Boolean(e.detail?.checked ?? e.target?.checked))
              }
              aria-label="管理本地 MCP 服务端口开启状态"
            />
          </SettingRow>

          <div className="flex flex-wrap items-center gap-2">
            {refreshing || waiting ? (
              <LoadingIndicator size={16} aria-label="正在读取 MCP 服务状态" />
            ) : view?.running ? (
              <Chip size="sm" variant="soft" color="success">
                端口已开启 ({port})
              </Chip>
            ) : (
              <Chip size="sm" variant="soft" color="default">
                已关闭
              </Chip>
            )}
            <Button
              size="xs"
              variant="text"
              disabled={refreshing || toggling}
              onClick={() => void reload()}
            >
              {refreshing ? <LoadingIndicator size={14} /> : <LuRefreshCw aria-hidden="true" />}
              刷新状态
            </Button>
          </div>

          {view?.running && (
            <div className="grid gap-2 rounded-inner bg-surface-secondary p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs font-medium text-muted">
                  服务端点 URL（Streamable HTTP / SSE）
                </span>
                <Button
                  size="xs"
                  variant="text"
                  onClick={() => void handleCopy(endpoint, "MCP 服务端点 URL ")}
                >
                  <LuCopy aria-hidden="true" />
                  复制 URL
                </Button>
              </div>
              <code className="select-all break-all rounded-inner bg-background px-3 py-2 font-mono text-xs text-foreground">
                {endpoint}
              </code>
              <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted">
                <span className="flex items-center gap-1">
                  <LuShieldCheck aria-hidden="true" className="size-3.5 shrink-0" />
                  仅监听回环地址 127.0.0.1
                </span>
                <span className="flex items-center gap-1">
                  <LuNetwork aria-hidden="true" className="size-3.5 shrink-0" />
                  独立高位端口：{port}
                </span>
              </div>
            </div>
          )}

          <div className="card-group">
            <h3 className="card-group-title">安全与会话</h3>
            <p className="text-xs leading-relaxed text-muted">
              服务直接共享本应用内存中的活动会话与凭据（统一认证登录态、网络连接、选课上下文）。
              由于只监听本机回环地址，外部网络无法访问，也不需要在外部重复输入账号密码。
            </p>
          </div>
        </div>
      </Card>

      {!onboarding && (
        <>
          <Card variant="outlined">
            <div slot="headline">
              <span className="inline-flex items-center gap-2">
                <LuBot aria-hidden="true" className="size-4" />
                指导 AI Agent 安装该 MCP 服务
              </span>
            </div>
            <div slot="subhead">
              复制下方提示词发给 Claude Code、Cursor、Windsurf 或其它 AI 编程助手，即可完成 MCP
              服务的识别与配置。
            </div>
            <div className="grid gap-3">
              <div>
                <Button
                  size="xs"
                  variant="filled"
                  onClick={() => void handleCopy(agentPrompt, "指导 Agent 安装的提示词")}
                >
                  <LuCopy aria-hidden="true" />
                  复制安装提示词
                </Button>
              </div>
              <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all rounded-inner bg-surface-secondary p-3 font-mono text-xs leading-relaxed text-foreground">
                {agentPrompt}
              </pre>
            </div>
          </Card>

          <Card variant="outlined">
            <div slot="headline">
              <span className="inline-flex items-center gap-2">
                <LuFileCode aria-hidden="true" className="size-4" />
                配置文件片段 (Claude Desktop / Cursor)
              </span>
            </div>
            <div slot="subhead">
              手动配置到 Claude Desktop 的 <code>claude_desktop_config.json</code> 或 Cursor 的 MCP
              设置中。
            </div>
            <div className="grid gap-3">
              <div>
                <Button
                  size="xs"
                  variant="outlined"
                  onClick={() => void handleCopy(jsonConfig, "mcpServers JSON 配置")}
                >
                  <LuCopy aria-hidden="true" />
                  复制 JSON
                </Button>
              </div>
              <pre className="overflow-x-auto rounded-inner bg-surface-secondary p-3 font-mono text-xs text-foreground">
                {jsonConfig}
              </pre>
            </div>
          </Card>
        </>
      )}
    </>
  );
}
