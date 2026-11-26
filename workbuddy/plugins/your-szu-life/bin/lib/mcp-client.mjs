// Minimal MCP Streamable HTTP client used by the WorkBuddy stdio bridge.
//
// The desktop app exposes:
//   POST http://127.0.0.1:<port>/mcp
//   Accept: application/json, text/event-stream
// Responses may be JSON or SSE; a session id is returned on initialize.

import net from "node:net";

import {
  EXPERT_VERSION,
  MCP_BRIDGE_NAME,
  MCP_LEGACY_PROTOCOL_VERSION,
  MCP_PROTOCOL_VERSION,
} from "./constants.mjs";

export class McpError extends Error {
  constructor(message, options = {}) {
    super(message);
    this.name = "McpError";
    this.code = options.code;
    this.status = options.status;
  }
}

export class SseDecoder {
  constructor() {
    this.buffer = "";
  }

  push(chunk) {
    this.buffer = (this.buffer + chunk).replace(/\r\n/g, "\n");
    const events = [];
    let boundary = this.buffer.indexOf("\n\n");
    while (boundary >= 0) {
      const raw = this.buffer.slice(0, boundary);
      this.buffer = this.buffer.slice(boundary + 2);
      const event = parseSseEvent(raw);
      if (event) events.push(event);
      boundary = this.buffer.indexOf("\n\n");
    }
    return events;
  }
}

export function parseSseEvent(raw) {
  let event = "message";
  const data = [];
  for (const line of String(raw).split(/\r?\n/)) {
    if (!line || line.startsWith(":")) continue;
    const colon = line.indexOf(":");
    const field = colon === -1 ? line : line.slice(0, colon);
    let value = colon === -1 ? "" : line.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "event") event = value;
    else if (field === "data") data.push(value);
  }
  if (!data.length) return null;
  return { event, data: data.join("\n") };
}

/** Parse a complete SSE body; useful in tests and for non-streaming responses. */
export function parseSseMessages(text) {
  const decoder = new SseDecoder();
  const events = decoder.push(String(text).replace(/\r\n/g, "\n"));
  const messages = [];
  for (const event of events) {
    try {
      const value = JSON.parse(event.data);
      if (Array.isArray(value)) messages.push(...value);
      else messages.push(value);
    } catch {
      // Ignore non-JSON SSE payloads (e.g. comments/keepalives).
    }
  }
  return messages;
}

export class McpHttpClient {
  constructor(url, options = {}) {
    this.url = url;
    this.requestTimeoutMs = options.requestTimeoutMs ?? 300_000;
    this.sessionId = null;
    this.protocolVersion = null;
    this.nextId = 1;
    this.ready = false;
    this.serverInfo = null;
  }

  async initialize() {
    if (this.ready) return this.serverInfo;
    const attempts = [MCP_PROTOCOL_VERSION, MCP_LEGACY_PROTOCOL_VERSION];
    let lastError = null;
    for (const protocolVersion of attempts) {
      const id = this.nextId++;
      const message = {
        jsonrpc: "2.0",
        id,
        method: "initialize",
        params: {
          protocolVersion,
          capabilities: {},
          clientInfo: { name: MCP_BRIDGE_NAME, version: EXPERT_VERSION },
        },
      };
      try {
        const result = await this.#post(message, {
          timeoutMs: Math.min(8_000, this.requestTimeoutMs),
        });
        if (!result) throw new McpError("MCP initialize 没有返回结果");
        this.protocolVersion = result.protocolVersion ?? protocolVersion;
        this.serverInfo = result.serverInfo ?? null;
        this.ready = true;
        await this.#post(
          { jsonrpc: "2.0", method: "notifications/initialized" },
          { timeoutMs: 3_000, notification: true },
        );
        return this.serverInfo;
      } catch (error) {
        lastError = error;
        this.ready = false;
        this.sessionId = null;
      }
    }
    throw lastError ?? new McpError("MCP initialize 失败");
  }

  async request(method, params, options = {}) {
    await this.initialize();
    const id = this.nextId++;
    try {
      return await this.#post(
        { jsonrpc: "2.0", id, method, params },
        { timeoutMs: options.timeoutMs ?? this.requestTimeoutMs },
      );
    } catch (error) {
      // Only retry transport/session failures. Never replay a tool call after a
      // server-side error because it may be a destructive operation.
      const retryable = error instanceof McpError && (error.status === 400 || error.status === 404);
      if (!retryable || options.noRetry) throw error;
      this.ready = false;
      this.sessionId = null;
      await this.initialize();
      return await this.#post(
        { jsonrpc: "2.0", id: this.nextId++, method, params },
        { timeoutMs: options.timeoutMs ?? this.requestTimeoutMs },
      );
    }
  }

  async listTools() {
    const result = await this.request("tools/list", {});
    return result?.tools ?? [];
  }

  async callTool(name, args = {}) {
    return await this.request(
      "tools/call",
      { name, arguments: args ?? {} },
      { timeoutMs: 300_000 },
    );
  }

  async listResources() {
    const result = await this.request("resources/list", {});
    return result?.resources ?? [];
  }

  async readResource(uri) {
    return await this.request("resources/read", { uri }, { timeoutMs: 60_000 });
  }

  async #post(message, options = {}) {
    const { timeoutMs = this.requestTimeoutMs, notification = false } = options;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const headers = {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
        "user-agent": `${MCP_BRIDGE_NAME}/${EXPERT_VERSION}`,
        connection: "close",
      };
      if (this.sessionId) headers["mcp-session-id"] = this.sessionId;
      if (this.protocolVersion) {
        headers["mcp-protocol-version"] = this.protocolVersion;
      }

      const response = await fetch(this.url, {
        method: "POST",
        headers,
        body: JSON.stringify(message),
        signal: controller.signal,
      });

      const sessionId = response.headers.get("mcp-session-id");
      if (sessionId) this.sessionId = sessionId;

      if (response.status === 202 || response.status === 204) return null;
      if (!response.ok) {
        const text = await response.text().catch(() => "");
        throw new McpError(`MCP HTTP ${response.status}${text ? `：${text.slice(0, 500)}` : ""}`, {
          status: response.status,
        });
      }
      if (notification || message.id === undefined) {
        await response.body?.cancel?.().catch?.(() => {});
        return null;
      }

      const contentType = response.headers.get("content-type") ?? "";
      if (contentType.includes("text/event-stream")) {
        return await readSseResponse(response, message.id);
      }

      const text = await response.text();
      if (!text.trim()) return null;
      let parsed;
      try {
        parsed = JSON.parse(text);
      } catch (error) {
        throw new McpError(`MCP 返回了无法解析的 JSON: ${String(error)}`);
      }
      const messages = Array.isArray(parsed) ? parsed : [parsed];
      const match = messages.find((entry) => entry && entry.id === message.id);
      if (!match) return null;
      return unwrap(match);
    } catch (error) {
      if (error?.name === "AbortError") {
        throw new McpError("MCP 请求超时", { status: undefined });
      }
      throw error;
    } finally {
      clearTimeout(timer);
    }
  }
}

async function readSseResponse(response, expectedId) {
  if (!response.body) return null;
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const sse = new SseDecoder();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) return null;
      const events = sse.push(decoder.decode(value, { stream: true }));
      for (const event of events) {
        if (event.data === "[DONE]") continue;
        let parsed;
        try {
          parsed = JSON.parse(event.data);
        } catch {
          continue;
        }
        const messages = Array.isArray(parsed) ? parsed : [parsed];
        const match = messages.find((entry) => entry && entry.id === expectedId);
        if (match) {
          await reader.cancel().catch(() => {});
          return unwrap(match);
        }
      }
    }
  } catch (error) {
    if (error?.name === "AbortError") throw error;
    throw new McpError(`读取 MCP SSE 响应失败: ${String(error)}`);
  }
}

function unwrap(message) {
  if (message.error) {
    throw new McpError(message.error.message || "MCP 工具调用失败", { code: message.error.code });
  }
  return message.result;
}

/** TCP-level connect probe with a short timeout. */
export function probeTcp(port, host = "127.0.0.1", timeoutMs = 180) {
  return new Promise((resolve) => {
    const socket = net.connect({ port, host });
    let settled = false;
    const finish = (ok) => {
      if (settled) return;
      settled = true;
      socket.destroy();
      resolve(ok);
    };
    socket.setTimeout(timeoutMs);
    socket.once("connect", () => finish(true));
    socket.once("timeout", () => finish(false));
    socket.once("error", () => finish(false));
  });
}

/**
 * Verify that a URL speaks MCP and belongs to the app (serverInfo.name).
 * Returns the initialized client so callers can reuse the session.
 */
export async function probeMcp(baseUrl, options = {}) {
  const { timeoutMs = 2_000, requireAppIdentity = true } = options;
  const client = new McpHttpClient(baseUrl, { requestTimeoutMs: timeoutMs });
  const info = await client.initialize();
  if (requireAppIdentity && info?.name !== "your-szu-life") {
    throw new McpError(`端口 ${baseUrl} 上的服务不是泥深 MCP`);
  }
  return { client, info, url: baseUrl };
}
