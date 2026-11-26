import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import readline from "node:readline";
import { fileURLToPath } from "node:url";

import {
  createFakeInstall,
  createTempDataHome,
  startMockMcpServer,
  writeDiscovery,
} from "./helpers.mjs";

const bridgeEntry = fileURLToPath(
  new URL("../plugins/your-szu-life/bin/szu-life.mjs", import.meta.url),
);

function startBridge(env) {
  const child = spawn(process.execPath, [bridgeEntry, "mcp"], {
    env: { ...process.env, ...env },
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  const pending = new Map();
  const reader = readline.createInterface({ input: child.stdout });
  reader.on("line", (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message.id !== undefined && pending.has(message.id)) {
      const { resolve, timer } = pending.get(message.id);
      pending.delete(message.id);
      clearTimeout(timer);
      resolve(message);
    }
  });

  const request = (id, method, params) =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`bridge request ${method} timed out`));
      }, 10_000);
      pending.set(id, { resolve, timer });
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });

  const notify = (method, params) => {
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method, params })}\n`);
  };

  return { child, request, notify };
}

test("stdio bridge initializes, lists management tools, and forwards calls", async () => {
  const dataHome = await createTempDataHome();
  const mock = await startMockMcpServer();
  const bridge = startBridge({
    XDG_DATA_HOME: dataHome,
    APPDATA: dataHome,
    LOCALAPPDATA: dataHome,
  });

  try {
    await createFakeInstall(dataHome, { version: "0.19.0" });
    await writeDiscovery(dataHome, {
      schemaVersion: 1,
      pid: process.pid,
      app: { version: "0.19.0" },
      mcp: {
        enabled: true,
        running: true,
        port: mock.port,
        preferredPort: mock.port,
        endpoint: mock.url,
      },
      onboarding: {
        available: true,
        needsSetup: false,
        currentStep: null,
        steps: [],
      },
      auth: { state: "authenticated", loggedIn: true },
      uooc: { session: "ready", loggedIn: true },
    });

    const initialize = await bridge.request(1, "initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "test", version: "1" },
    });
    assert.equal(initialize.result.serverInfo.name, "your-szu-life-workbuddy");
    bridge.notify("notifications/initialized");

    const list = await bridge.request(2, "tools/list", {});
    const names = list.result.tools.map((tool) => tool.name);
    assert.ok(names.includes("szu_life_status"));
    assert.ok(names.includes("szu_life_ensure_ready"));
    assert.ok(names.includes("szu_life_call"));
    assert.ok(names.includes("echo"));
    assert.ok(names.includes("todos_list"));

    const forwarded = await bridge.request(3, "tools/call", {
      name: "szu_life_call",
      arguments: { tool: "echo", arguments: { value: "bridge" } },
    });
    assert.equal(forwarded.result.isError, false);
    assert.equal(forwarded.result.content[0].text, '{"value":"bridge"}');

    const status = await bridge.request(4, "tools/call", {
      name: "szu_life_status",
      arguments: {},
    });
    assert.equal(status.result.isError, false);
    const payload = JSON.parse(status.result.content[0].text);
    assert.equal(payload.installed, true);
    assert.equal(payload.running, true);
    assert.equal(payload.mcp.reachable, true);
  } finally {
    bridge.child.stdin.end();
    await new Promise((resolve) => {
      const timer = setTimeout(() => {
        bridge.child.kill("SIGKILL");
        resolve();
      }, 5_000);
      bridge.child.once("exit", () => {
        clearTimeout(timer);
        resolve();
      });
    });
    await mock.close();
  }
});
