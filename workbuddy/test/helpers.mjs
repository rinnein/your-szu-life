// Test helpers: a local mock Streamable HTTP MCP server and temp app-data setup.

import http from "node:http";
import path from "node:path";
import { chmod, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import os from "node:os";

export const MOCK_TOOLS = [
  {
    name: "echo",
    description: "Echo the arguments",
    inputSchema: {
      type: "object",
      properties: { value: { type: "string" } },
    },
  },
  {
    name: "todos_list",
    description: "List todos",
    inputSchema: { type: "object", properties: {} },
  },
];

export async function startMockMcpServer(options = {}) {
  const {
    sse = false,
    appName = "your-szu-life",
    appVersion = "0.19.0",
    remoteStatus = {
      onboarding: {
        available: true,
        needsSetup: false,
        currentStep: null,
        steps: [],
      },
      auth: { state: "authenticated", loggedIn: true },
      uooc: { session: "ready", loggedIn: true },
    },
    supportsIntegrationStatus = true,
  } = options;

  let sessionCounter = 0;
  const server = http.createServer(async (request, response) => {
    if (request.method !== "POST" || !request.url?.startsWith("/mcp")) {
      response.writeHead(404).end();
      return;
    }
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    let message;
    try {
      message = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    } catch {
      response.writeHead(400).end("bad json");
      return;
    }

    const sessionId = request.headers["mcp-session-id"] || `session-${sessionCounter}`;
    const send = (payload, status = 200) => {
      response.writeHead(status, {
        "content-type": "application/json",
        "mcp-session-id": sessionId,
      });
      response.end(JSON.stringify(payload));
    };
    const sendSse = (payload) => {
      response.writeHead(200, {
        "content-type": "text/event-stream",
        "mcp-session-id": sessionId,
        "cache-control": "no-cache",
      });
      response.end(`event: message\ndata: ${JSON.stringify(payload)}\n\n`);
    };
    const respond = (payload) => (sse ? sendSse(payload) : send(payload));

    if (message.method === "initialize") {
      sessionCounter += 1;
      respond({
        jsonrpc: "2.0",
        id: message.id,
        result: {
          protocolVersion: message.params?.protocolVersion ?? "2025-06-18",
          capabilities: { tools: {}, resources: {} },
          serverInfo: { name: appName, version: appVersion },
        },
      });
      return;
    }
    if (message.method === "notifications/initialized") {
      response.writeHead(202, { "mcp-session-id": sessionId }).end();
      return;
    }
    if (message.method === "tools/list") {
      respond({ jsonrpc: "2.0", id: message.id, result: { tools: MOCK_TOOLS } });
      return;
    }
    if (message.method === "tools/call") {
      const name = message.params?.name;
      const args = message.params?.arguments ?? {};
      if (name === "echo") {
        respond({
          jsonrpc: "2.0",
          id: message.id,
          result: {
            content: [{ type: "text", text: JSON.stringify(args) }],
            isError: false,
          },
        });
        return;
      }
      if (name === "integration_get_status" && supportsIntegrationStatus) {
        respond({
          jsonrpc: "2.0",
          id: message.id,
          result: {
            content: [
              {
                type: "text",
                text: JSON.stringify({
                  schemaVersion: 1,
                  app: { version: appVersion },
                  mcp: { enabled: true, running: true },
                  ...remoteStatus,
                }),
              },
            ],
            isError: false,
          },
        });
        return;
      }
      if (name === "onboarding_get_status") {
        respond({
          jsonrpc: "2.0",
          id: message.id,
          result: {
            content: [
              {
                type: "text",
                text: JSON.stringify({
                  needsSetup: remoteStatus.onboarding?.needsSetup ?? false,
                  currentStep: remoteStatus.onboarding?.currentStep ?? null,
                  steps: remoteStatus.onboarding?.steps ?? [],
                }),
              },
            ],
            isError: false,
          },
        });
        return;
      }
      if (name === "auth_get_status") {
        respond({
          jsonrpc: "2.0",
          id: message.id,
          result: {
            content: [
              {
                type: "text",
                text: JSON.stringify({
                  authenticated: { studentId: "test", name: "test" },
                }),
              },
            ],
            isError: false,
          },
        });
        return;
      }
      respond({
        jsonrpc: "2.0",
        id: message.id,
        result: {
          content: [{ type: "text", text: "unknown tool" }],
          isError: true,
        },
      });
      return;
    }
    if (message.method === "resources/list") {
      respond({ jsonrpc: "2.0", id: message.id, result: { resources: [] } });
      return;
    }
    if (message.method === "resources/read") {
      respond({
        jsonrpc: "2.0",
        id: message.id,
        result: {
          contents: [
            {
              uri: message.params?.uri,
              mimeType: "application/json",
              text: JSON.stringify({ session: remoteStatus.uooc?.session ?? "ready" }),
            },
          ],
        },
      });
      return;
    }
    respond({
      jsonrpc: "2.0",
      id: message.id,
      error: { code: -32601, message: "unknown method" },
    });
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  return {
    server,
    port: address.port,
    url: `http://127.0.0.1:${address.port}/mcp`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

export async function createTempDataHome(prefix = "szu-life-test-") {
  const directory = await mkdtemp(path.join(os.tmpdir(), prefix));
  process.env.XDG_DATA_HOME = directory;
  process.env.APPDATA = directory;
  process.env.LOCALAPPDATA = directory;
  return directory;
}

export async function writeDiscovery(dataHome, value) {
  const directory = path.join(dataHome, "in.rinne.your-szu-life");
  await mkdir(directory, { recursive: true });
  await writeFile(path.join(directory, "integration.json"), JSON.stringify(value, null, 2));
}

export async function createFakeInstall(dataHome, options = {}) {
  const directory = path.join(dataHome, "in.rinne.your-szu-life");
  await mkdir(directory, { recursive: true });
  const binary = path.join(dataHome, "your-szu-life", "your-szu-life");
  await mkdir(path.dirname(binary), { recursive: true });
  await writeFile(binary, "#!/bin/sh\nexit 0\n");
  await chmod(binary, 0o755);
  await writeFile(
    path.join(directory, "install.json"),
    JSON.stringify({
      schemaVersion: 1,
      version: options.version ?? "0.19.0",
      channel: options.channel ?? "stable",
      format: "appimage",
      source: "test",
      path: binary,
      installedAt: new Date().toISOString(),
    }),
  );
  return { binary, directory };
}

export async function readJson(file) {
  return JSON.parse(await readFile(file, "utf8"));
}
