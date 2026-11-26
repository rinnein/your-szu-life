import test from "node:test";
import assert from "node:assert/strict";

import {
  McpHttpClient,
  SseDecoder,
  parseSseMessages,
  probeMcp,
  probeTcp,
} from "../plugins/your-szu-life/bin/lib/mcp-client.mjs";
import { startMockMcpServer } from "./helpers.mjs";

test("SseDecoder handles CRLF and split chunks", () => {
  const decoder = new SseDecoder();
  assert.deepEqual(decoder.push("event: message\r\nda"), []);
  const events = decoder.push('ta: {"id":1}\r\n\r\n');
  assert.equal(events.length, 1);
  assert.equal(events[0].event, "message");
  assert.equal(events[0].data, '{"id":1}');
});

test("parseSseMessages ignores non-JSON keepalives", () => {
  const messages = parseSseMessages(
    'event: message\ndata: {"id":1,"result":{"ok":true}}\n\n' + "event: ping\ndata: not-json\n\n",
  );
  assert.equal(messages.length, 1);
  assert.equal(messages[0].id, 1);
});

test("JSON mock: initialize, tools/list, tools/call", async () => {
  const mock = await startMockMcpServer();
  try {
    const probe = await probeMcp(mock.url);
    assert.equal(probe.info.name, "your-szu-life");
    const tools = await probe.client.listTools();
    assert.ok(tools.some((tool) => tool.name === "echo"));
    const result = await probe.client.callTool("echo", { value: "hello" });
    assert.equal(result.isError, false);
    assert.equal(result.content[0].text, '{"value":"hello"}');
  } finally {
    await mock.close();
  }
});

test("SSE mock: probe and tools/list", async () => {
  const mock = await startMockMcpServer({ sse: true });
  try {
    const client = new McpHttpClient(mock.url);
    const info = await client.initialize();
    assert.equal(info.name, "your-szu-life");
    const tools = await client.listTools();
    assert.equal(tools.length, 2);
  } finally {
    await mock.close();
  }
});

test("probeMcp rejects a different server identity", async () => {
  const mock = await startMockMcpServer({ appName: "not-your-app" });
  try {
    await assert.rejects(() => probeMcp(mock.url), /不是泥深 MCP/);
  } finally {
    await mock.close();
  }
});

test("probeTcp distinguishes open and closed ports", async () => {
  const mock = await startMockMcpServer();
  try {
    assert.equal(await probeTcp(mock.port), true);
  } finally {
    await mock.close();
  }
  assert.equal(await probeTcp(mock.port), false);
});
