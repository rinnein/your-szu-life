// Launch/open the desktop app and drive the first-run state machine.

import { spawn } from "node:child_process";

import { DEEP_LINKS } from "./constants.mjs";
import { collectStatus } from "./status.mjs";
import { findAppBinary, readInstallMeta, readIntegrationFile } from "./paths.mjs";
import { findAppProcess, isPidAlive, waitFor } from "./processes.mjs";
import { log, sleep } from "./util.mjs";

function spawnDetached(binary, args) {
  const child = spawn(binary, args, {
    detached: true,
    stdio: "ignore",
    windowsHide: true,
    shell: false,
  });
  child.unref();
  return child;
}

async function findInstalledBinary() {
  const installMeta = await readInstallMeta();
  return findAppBinary(installMeta?.value);
}

export async function launchApp(options = {}) {
  const { visible = false } = options;
  const existing = await findAppProcess();
  if (existing) {
    return { ok: true, alreadyRunning: true, pid: existing.pid };
  }

  const binary = await findInstalledBinary();
  if (!binary) {
    return {
      ok: false,
      error: "没有找到泥深可执行文件，请先安装。",
      nextAction: "install",
    };
  }

  const args = visible ? [] : ["--silent"];
  log(`启动泥深: ${binary} ${args.join(" ")}`);
  spawnDetached(binary, args);

  const started = await waitFor(
    async () => {
      const processInfo = await findAppProcess();
      if (processInfo) return processInfo;
      const discovery = await readIntegrationFile();
      if (discovery?.value?.pid && isPidAlive(discovery.value.pid)) {
        return discovery.value;
      }
      return null;
    },
    { timeoutMs: 20_000, intervalMs: 500 },
  );

  if (!started) {
    return {
      ok: false,
      error: "泥深没有在 20 秒内启动成功，请检查桌面会话或手动打开。",
    };
  }
  return {
    ok: true,
    pid: started.pid ?? null,
    silent: !visible,
  };
}

export async function openApp(target) {
  const links = {
    home: DEEP_LINKS.home,
    settings: DEEP_LINKS.settings,
    "mcp-settings": DEEP_LINKS.mcpSettings,
    uooc: DEEP_LINKS.uooc,
  };
  const url = links[target];
  if (!url) {
    return {
      ok: false,
      error: `未知的打开目标：${target}。可用：${Object.keys(links).join("、")}`,
    };
  }
  const binary = await findInstalledBinary();
  if (!binary) {
    return {
      ok: false,
      error: "没有找到泥深可执行文件，请先安装。",
      nextAction: "install",
    };
  }

  log(`打开泥深页面: ${url}`);
  // Do not add --silent: the point of open is to bring the window forward.
  spawnDetached(binary, [url]);
  await sleep(1_500);
  return { ok: true, url };
}

/**
 * Bring the app to a usable state. Never installs. Opening a page is optional
 * so callers can first inspect nextAction and ask the user.
 */
export async function ensureReady(options = {}) {
  const { need = "general", allowScan = true, open = true, launch = true } = options;

  let snapshot = await collectStatus({ need, allowScan });
  let actionPerformed = null;

  if (snapshot.status.nextAction === "launch" && launch) {
    const launched = await launchApp({ visible: false });
    actionPerformed = "launch";
    if (!launched.ok) {
      return {
        ok: false,
        actionPerformed,
        error: launched.error,
        status: snapshot.status,
      };
    }
    // Wait briefly for the newly started process to write integration.json so
    // the next state decision is based on fresh data, not a stale/missing file.
    if (launched.alreadyRunning || !launched.pid) {
      await sleep(800);
    } else {
      const targetPid = launched.pid;
      await waitFor(
        async () => {
          const found = await readIntegrationFile();
          return found?.value?.pid === targetPid ? found.value : null;
        },
        { timeoutMs: 4_000, intervalMs: 250 },
      );
    }
    snapshot = await collectStatus({ need, allowScan });
  }

  const { status } = snapshot;
  if (status.nextAction === "onboarding" || status.nextAction === "auth") {
    if (open) {
      await openApp("home");
      actionPerformed = "open-home";
    }
    return {
      ok: true,
      actionPerformed,
      nextAction: status.nextAction,
      status,
    };
  }

  if (status.nextAction === "enable-mcp" || status.nextAction === "mcp-unreachable") {
    if (open) {
      await openApp("mcp-settings");
      actionPerformed = "open-mcp-settings";
    }
    return {
      ok: true,
      actionPerformed,
      nextAction: status.nextAction,
      status,
    };
  }

  if (status.nextAction === "uooc-login") {
    if (open) {
      await openApp("uooc");
      actionPerformed = "open-uooc";
    }
    return {
      ok: true,
      actionPerformed,
      nextAction: "uooc-login",
      status,
    };
  }

  if (status.nextAction === "launch") {
    return {
      ok: false,
      actionPerformed,
      error: "泥深仍未运行，无法继续。",
      status,
    };
  }

  if (status.nextAction === "ready" && open && actionPerformed === null) {
    // Nothing to open; the app is ready.
  }

  return {
    ok: true,
    actionPerformed,
    nextAction: status.nextAction,
    status,
  };
}
