import test from "node:test";
import assert from "node:assert/strict";

import { collectStatus } from "../plugins/your-szu-life/bin/lib/status.mjs";
import {
  createFakeInstall,
  createTempDataHome,
  startMockMcpServer,
  writeDiscovery,
} from "./helpers.mjs";

function baseDiscovery(port, overrides = {}) {
  return {
    schemaVersion: 1,
    updatedAtMs: Date.now(),
    startedAtMs: Date.now(),
    pid: process.pid,
    app: { version: "0.19.0", platform: process.platform, arch: process.arch },
    mcp: {
      enabled: true,
      running: true,
      port,
      preferredPort: port,
      endpoint: `http://127.0.0.1:${port}/mcp`,
    },
    onboarding: {
      available: true,
      needsSetup: false,
      currentStep: null,
      steps: [],
    },
    auth: { state: "authenticated", loggedIn: true },
    uooc: { session: "ready", loggedIn: true },
    ...overrides,
  };
}

test("collectStatus reports ready with a live app and mock MCP", async () => {
  const dataHome = await createTempDataHome();
  const mock = await startMockMcpServer();
  try {
    await createFakeInstall(dataHome, { version: "0.19.0" });
    await writeDiscovery(dataHome, baseDiscovery(mock.port));
    const { status } = await collectStatus({ allowScan: false });
    assert.equal(status.installed, true);
    assert.equal(status.running, true);
    assert.equal(status.mcp.reachable, true);
    assert.equal(status.mcp.port, mock.port);
    assert.equal(status.nextAction, "ready");
    assert.equal(status.onboarding.needsSetup, false);
    assert.equal(status.auth.loggedIn, true);
    assert.equal(status.uooc.loggedIn, true);
  } finally {
    await mock.close();
  }
});

test("collectStatus asks to enable MCP when the switch is off", async () => {
  const dataHome = await createTempDataHome();
  const mock = await startMockMcpServer();
  try {
    await createFakeInstall(dataHome);
    await writeDiscovery(
      dataHome,
      baseDiscovery(mock.port, {
        mcp: {
          enabled: false,
          running: false,
          port: null,
          preferredPort: mock.port,
          endpoint: null,
        },
      }),
    );
    const { status } = await collectStatus({ allowScan: false });
    assert.equal(status.nextAction, "enable-mcp");
    assert.equal(status.mcp.enabled, false);
  } finally {
    await mock.close();
  }
});

test("collectStatus prioritises onboarding and auth", async () => {
  const dataHome = await createTempDataHome();
  const mock = await startMockMcpServer();
  try {
    await createFakeInstall(dataHome);
    await writeDiscovery(
      dataHome,
      baseDiscovery(mock.port, {
        onboarding: {
          available: true,
          needsSetup: true,
          currentStep: "welcome",
          steps: [],
        },
      }),
    );
    let { status } = await collectStatus({ allowScan: false });
    assert.equal(status.nextAction, "onboarding");

    await writeDiscovery(
      dataHome,
      baseDiscovery(mock.port, {
        auth: { state: "unauthenticated", loggedIn: false },
      }),
    );
    ({ status } = await collectStatus({ allowScan: false }));
    assert.equal(status.nextAction, "auth");
  } finally {
    await mock.close();
  }
});

test("collectStatus reports uooc-login only when needed", async () => {
  const dataHome = await createTempDataHome();
  const mock = await startMockMcpServer();
  try {
    await createFakeInstall(dataHome);
    await writeDiscovery(
      dataHome,
      baseDiscovery(mock.port, {
        uooc: { session: "loginRequired", loggedIn: false },
      }),
    );
    let { status } = await collectStatus({ allowScan: false });
    assert.equal(status.nextAction, "ready");
    ({ status } = await collectStatus({ allowScan: false, need: "uooc" }));
    assert.equal(status.nextAction, "uooc-login");
  } finally {
    await mock.close();
  }
});

test("collectStatus falls back to legacy MCP tools without integration.json", async () => {
  const dataHome = await createTempDataHome();
  const mock = await startMockMcpServer({
    supportsIntegrationStatus: false,
    remoteStatus: {
      onboarding: {
        available: true,
        needsSetup: false,
        currentStep: null,
        steps: [],
      },
      auth: { state: "authenticated", loggedIn: true },
      uooc: { session: "loginRequired", loggedIn: false },
    },
  });
  try {
    await createFakeInstall(dataHome);
    // Endpoint only; no onboarding/auth/uooc fields.
    await writeDiscovery(dataHome, {
      schemaVersion: 1,
      pid: process.pid,
      mcp: {
        enabled: true,
        running: true,
        port: mock.port,
        preferredPort: mock.port,
        endpoint: mock.url,
      },
    });
    const { status } = await collectStatus({ allowScan: false });
    assert.equal(status.mcp.reachable, true);
    assert.equal(status.onboarding.available, true);
    assert.equal(status.auth.loggedIn, true);
    assert.equal(status.uooc.session, "loginRequired");
  } finally {
    await mock.close();
  }
});
