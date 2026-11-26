// stdio MCP bridge used by WorkBuddy experts.
//
// The bridge itself always starts, so the expert can chat before the desktop
// app is installed. It exposes a few szu_life_* management tools and proxies
// the app's full MCP tool/resource surface once the app is reachable.

import readline from "node:readline";

import { APP_NAME, MCP_BRIDGE_NAME, MCP_PROTOCOL_VERSION, EXPERT_VERSION } from "./constants.mjs";
import { collectStatus, discoverMcp, mcpEndpoint } from "./status.mjs";
import { ensureReady, launchApp, openApp } from "./ensure.mjs";
import { readIntegrationFile } from "./paths.mjs";
import { McpError } from "./mcp-client.mjs";
import { log } from "./util.mjs";

const MANAGEMENT_TOOLS = [
  {
    name: "szu_life_status",
    description:
      "检查泥深桌面端是否已安装、是否运行、MCP 是否开启/可连接、初始化与统一认证/UOOC 登录状态；返回下一步 nextAction。不含姓名、学号、Cookie、密码或密钥。",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "szu_life_ensure_ready",
    description:
      "按状态机把泥深带到可用状态：必要时静默启动、打开 MCP 设置页或首页初始化向导。不会安装。参数 need=uooc 时会额外检查 UOOC 登录。",
    inputSchema: {
      type: "object",
      properties: {
        need: {
          type: "string",
          enum: ["general", "uooc"],
          description: "检查目标：general 或 uooc",
        },
        open: {
          type: "boolean",
          description: "需要用户操作时是否自动打开对应页面，默认 true",
        },
      },
      additionalProperties: false,
    },
  },
  {
    name: "szu_life_launch",
    description: "启动泥深桌面端。默认使用 --silent 隐藏在托盘；visible=true 时显示窗口。",
    inputSchema: {
      type: "object",
      properties: {
        visible: {
          type: "boolean",
          description: "是否显示主窗口，默认 false",
        },
      },
      additionalProperties: false,
    },
  },
  {
    name: "szu_life_open",
    description: "打开泥深窗口并导航到指定页面：home、settings、mcp-settings、uooc。",
    inputSchema: {
      type: "object",
      properties: {
        target: {
          type: "string",
          enum: ["home", "settings", "mcp-settings", "uooc"],
        },
      },
      required: ["target"],
      additionalProperties: false,
    },
  },
  {
    name: "szu_life_list_tools",
    description:
      "列出泥深应用当前通过 MCP 暴露的工具（名称、描述、参数 schema）。原生工具列表尚未刷新时用它发现工具。",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "szu_life_call",
    description:
      "调用泥深应用的任意 MCP 工具。参数 {tool, arguments}。用于原生工具列表尚未刷新、或模型看不到具体工具时。",
    inputSchema: {
      type: "object",
      properties: {
        tool: { type: "string", description: "工具名，例如 todos_list" },
        arguments: {
          type: "object",
          description: "工具参数对象，可为空",
          additionalProperties: true,
        },
      },
      required: ["tool"],
      additionalProperties: false,
    },
  },
  {
    name: "szu_life_read_resource",
    description:
      "读取泥深应用的 MCP 资源。uri 必须以 your-szu-life://data/ 开头，例如 your-szu-life://data/onboarding/status。",
    inputSchema: {
      type: "object",
      properties: { uri: { type: "string" } },
      required: ["uri"],
      additionalProperties: false,
    },
  },
  {
    name: "szu_life_mcp_url",
    description:
      "返回当前探测到的泥深 MCP HTTP 端点、端口、版本与发现来源。可用于在 WorkBuddy 中配置原生 HTTP 直连。",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
];

function textResult(text) {
  return { content: [{ type: "text", text }], isError: false };
}

function errorResult(message) {
  return { content: [{ type: "text", text: message }], isError: true };
}

function jsonResult(value) {
  return textResult(JSON.stringify(value, null, 2));
}

class BridgeState {
  constructor() {
    this.client = null;
    this.endpoint = null;
    this.connectedAt = 0;
    this.tools = [];
    this.toolsAt = 0;
    this.listChangedSent = false;
  }

  async ensureClient(options = {}) {
    const { scan = true } = options;
    if (this.client) return this.client;
    const discovery = await readIntegrationFile();
    const found = await discoverMcp({ data: discovery?.value ?? { mcp: {} } }, { allowScan: scan });
    if (!found.reachable || !found.client) return null;
    this.client = found.client;
    this.endpoint = found.url;
    this.connectedAt = Date.now();
    this.toolsAt = 0;
    this.sendListChanged();
    return this.client;
  }

  async listAppTools() {
    if (this.tools.length && Date.now() - this.toolsAt < 60_000) {
      return this.tools;
    }
    const client = await this.ensureClient({ scan: false });
    if (!client) return [];
    try {
      this.tools = await client.listTools();
      this.toolsAt = Date.now();
      return this.tools;
    } catch (error) {
      log(`读取应用工具列表失败: ${String(error)}`);
      this.invalidate();
      return [];
    }
  }

  invalidate() {
    this.client = null;
    this.endpoint = null;
    this.tools = [];
    this.toolsAt = 0;
    this.listChangedSent = false;
  }

  sendListChanged() {
    if (this.listChangedSent) return;
    this.listChangedSent = true;
    sendNotification("notifications/tools/list_changed", {});
  }

  endpointInfo() {
    return {
      reachable: Boolean(this.client),
      url: this.endpoint,
      port: this.endpoint ? Number(new URL(this.endpoint).port) : null,
      name: APP_NAME,
    };
  }
}

function sendRaw(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function sendNotification(method, params) {
  sendRaw({ jsonrpc: "2.0", method, params });
}

function sendResult(id, result) {
  sendRaw({ jsonrpc: "2.0", id, result });
}

function sendError(id, code, message) {
  sendRaw({ jsonrpc: "2.0", id, error: { code, message } });
}

function errorMessage(error) {
  if (error instanceof McpError) {
    return `${error.message}${error.code ? ` (code ${error.code})` : ""}`;
  }
  return String(error?.message ?? error);
}

async function handleToolCall(state, params) {
  const name = params?.name;
  const args = params?.arguments ?? {};

  if (name === "szu_life_status") {
    const { status } = await collectStatus({ allowScan: true });
    return jsonResult(status);
  }

  if (name === "szu_life_ensure_ready") {
    const result = await ensureReady({
      need: args.need === "uooc" ? "uooc" : "general",
      open: args.open !== false,
    });
    // Drop the heavy nested status? Keep it: the model needs nextAction details.
    return jsonResult(result);
  }

  if (name === "szu_life_launch") {
    const result = await launchApp({ visible: args.visible === true });
    return jsonResult(result);
  }

  if (name === "szu_life_open") {
    const result = await openApp(args.target);
    return jsonResult(result);
  }

  if (name === "szu_life_list_tools") {
    const client = await state.ensureClient({ scan: true });
    if (!client) {
      return errorResult(
        "泥深 MCP 尚未就绪。先调用 szu_life_ensure_ready，按 nextAction 完成安装/启动/开启 MCP/初始化。",
      );
    }
    const tools = await state.listAppTools();
    return jsonResult(
      tools.map((tool) => ({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema,
      })),
    );
  }

  if (name === "szu_life_call") {
    if (!args.tool || typeof args.tool !== "string") {
      return errorResult("缺少 tool 参数");
    }
    const client = await state.ensureClient({ scan: true });
    if (!client) {
      return errorResult(
        "泥深 MCP 尚未就绪。先调用 szu_life_ensure_ready，按 nextAction 完成安装/启动/开启 MCP/初始化。",
      );
    }
    try {
      return await client.callTool(args.tool, args.arguments ?? {});
    } catch (error) {
      state.invalidate();
      return errorResult(`调用 ${args.tool} 失败：${errorMessage(error)}`);
    }
  }

  if (name === "szu_life_read_resource") {
    const uri = args.uri;
    if (typeof uri !== "string" || !uri.startsWith("your-szu-life://data/")) {
      return errorResult("uri 必须以 your-szu-life://data/ 开头");
    }
    const client = await state.ensureClient({ scan: true });
    if (!client) {
      return errorResult("泥深 MCP 尚未就绪。");
    }
    try {
      const result = await client.readResource(uri);
      return jsonResult(result);
    } catch (error) {
      state.invalidate();
      return errorResult(`读取资源失败：${errorMessage(error)}`);
    }
  }

  if (name === "szu_life_mcp_url") {
    const info = await mcpEndpoint();
    return jsonResult({
      reachable: info.reachable,
      url: info.url,
      port: info.port,
      serverName: info.serverName,
      serverVersion: info.serverVersion,
      source: info.source,
    });
  }

  return errorResult(`未知的 szu_life_* 工具：${name}`);
}

async function handleToolsCall(state, params) {
  const name = params?.name;
  if (typeof name !== "string") {
    throw new McpError("tools/call 缺少 name");
  }
  if (name.startsWith("szu_life_")) {
    return await handleToolCall(state, params);
  }

  const client = await state.ensureClient({ scan: true });
  if (!client) {
    return errorResult(
      "泥深 MCP 尚未就绪。先调用 szu_life_ensure_ready，或使用 szu_life_call 转发。",
    );
  }
  try {
    return await client.callTool(name, params.arguments ?? {});
  } catch (error) {
    state.invalidate();
    return errorResult(`调用泥深工具 ${name} 失败：${errorMessage(error)}`);
  }
}

async function handleMessage(state, message) {
  const id = message.id;
  const method = message.method;

  if (message.jsonrpc !== "2.0" || typeof method !== "string") {
    if (id !== undefined) sendError(id, -32600, "无效的 JSON-RPC 请求");
    return;
  }

  try {
    switch (method) {
      case "initialize": {
        const requested = message.params?.protocolVersion;
        sendResult(id, {
          protocolVersion: typeof requested === "string" ? requested : MCP_PROTOCOL_VERSION,
          capabilities: {
            tools: { listChanged: true },
            resources: { subscribe: false, listChanged: false },
          },
          serverInfo: { name: MCP_BRIDGE_NAME, version: EXPERT_VERSION },
          instructions:
            "这是泥深桌面端的 WorkBuddy 桥。先调用 szu_life_status 或 szu_life_ensure_ready 检查安装/运行/MCP/初始化状态；按 nextAction 引导用户。泥深应用工具可原生调用；若列表尚未刷新，用 szu_life_list_tools + szu_life_call 转发。",
        });
        return;
      }
      case "notifications/initialized":
      case "notifications/cancelled":
      case "notifications/roots/list_changed":
        return;
      case "ping":
        sendResult(id, {});
        return;
      case "logging/setLevel":
        sendResult(id, {});
        return;
      case "tools/list": {
        const appTools = await state.listAppTools();
        sendResult(id, { tools: [...MANAGEMENT_TOOLS, ...appTools] });
        return;
      }
      case "tools/call": {
        const result = await handleToolsCall(state, message.params);
        sendResult(id, result);
        return;
      }
      case "resources/list": {
        const client = await state.ensureClient({ scan: false });
        if (!client) {
          sendResult(id, { resources: [] });
          return;
        }
        try {
          const resources = await client.listResources();
          sendResult(id, { resources });
        } catch {
          state.invalidate();
          sendResult(id, { resources: [] });
        }
        return;
      }
      case "resources/templates/list":
        sendResult(id, { resourceTemplates: [] });
        return;
      case "resources/read": {
        const uri = message.params?.uri;
        if (typeof uri !== "string" || !uri.startsWith("your-szu-life://data/")) {
          sendError(id, -32602, "uri 必须以 your-szu-life://data/ 开头");
          return;
        }
        const client = await state.ensureClient({ scan: true });
        if (!client) {
          sendError(id, -32000, "泥深 MCP 尚未就绪");
          return;
        }
        try {
          const result = await client.readResource(uri);
          sendResult(id, result);
        } catch (error) {
          state.invalidate();
          sendError(id, -32000, errorMessage(error));
        }
        return;
      }
      case "prompts/list":
        sendResult(id, { prompts: [] });
        return;
      default:
        sendError(id, -32601, `未知方法：${method}`);
    }
  } catch (error) {
    log(`处理 ${method} 失败: ${errorMessage(error)}`);
    if (id !== undefined) sendError(id, -32603, errorMessage(error));
  }
}

export async function runBridge() {
  const state = new BridgeState();
  const reader = readline.createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  });

  reader.on("line", (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    let message;
    try {
      message = JSON.parse(trimmed);
    } catch {
      log("忽略无法解析的 stdin 行");
      return;
    }
    void handleMessage(state, message);
  });

  reader.on("close", () => {
    process.exit(0);
  });

  // Keep the process alive until stdin closes.
  await new Promise(() => {});
}
