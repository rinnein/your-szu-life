#!/usr/bin/env node
// CLI entry for the your-szu-life WorkBuddy expert.
//
// stdout is reserved for machine-readable JSON (or MCP protocol frames in
// `mcp` mode). Human-readable progress goes to stderr.

import path from "node:path";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

import { EXPERT_VERSION } from "./lib/constants.mjs";
import { detectLinuxDistro, detectPlatform } from "./lib/platform.mjs";
import { compareVersions, resolveInstallPlan } from "./lib/releases.mjs";
import { archInstallScript, installAppImage, installCommandFor } from "./lib/install-linux.mjs";
import { installMsi } from "./lib/install-windows.mjs";
import { collectStatus } from "./lib/status.mjs";
import { ensureReady, launchApp, openApp } from "./lib/ensure.mjs";
import { runBridge } from "./lib/bridge.mjs";
import { discoverMcp } from "./lib/status.mjs";
import { mcpEndpoint } from "./lib/status.mjs";
import { readIntegrationFile } from "./lib/paths.mjs";
import { ConsentRequiredError, jsonOutput, log } from "./lib/util.mjs";

const HELP = `泥深 WorkBuddy 专家桥 ${EXPERT_VERSION}

用法:
  szu-life status [--json]
  szu-life ensure [--need general|uooc] [--no-open] [--json]
  szu-life install [--yes] [--dry-run] [--force] [--channel stable|canary] [--format auto|appimage]
  szu-life launch [--visible]
  szu-life open <home|settings|mcp-settings|uooc>
  szu-life mcp
  szu-life mcp-list
  szu-life mcp-call <tool> [json|-|@file]
  szu-life mcp-url
  szu-life version

Linux 安装策略:
  Debian 系  输出 deb 安装 bash 命令，由用户执行
  Arch 系    输出 AUR (your-szu-life-bin) 安装 bash 命令，由用户执行
  其它发行版 由 agent 安装 AppImage 到 XDG 用户目录（需要 --yes）

Windows: 下载 MSI，经 UAC 静默安装（需要 --yes）。
`;

function parseArgs(argv) {
  const positional = [];
  const flags = {};
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (!value.startsWith("--")) {
      positional.push(value);
      continue;
    }
    const equals = value.indexOf("=");
    if (equals > 0) {
      flags[value.slice(2, equals)] = value.slice(equals + 1);
      continue;
    }
    const key = value.slice(2);
    const next = argv[index + 1];
    if (next !== undefined && !next.startsWith("--")) {
      flags[key] = next;
      index += 1;
    } else {
      flags[key] = true;
    }
  }
  return { positional, flags };
}

function isTrue(value) {
  return value === true || value === "true" || value === "1";
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

async function readJsonArgument(value) {
  if (!value || value === "-") {
    const text = await readStdin();
    return text.trim() ? JSON.parse(text) : {};
  }
  if (value.startsWith("@")) {
    return JSON.parse(await readFile(value.slice(1), "utf8"));
  }
  return JSON.parse(value);
}

async function commandStatus(flags) {
  const need = flags.need === "uooc" ? "uooc" : "general";
  const { status } = await collectStatus({ need, allowScan: true });
  jsonOutput(status);
}

async function commandEnsure(flags) {
  const result = await ensureReady({
    need: flags.need === "uooc" ? "uooc" : "general",
    open: !isTrue(flags["no-open"]),
  });
  jsonOutput(result);
  if (!result.ok) process.exitCode = 1;
}

function planSummary(plan) {
  return {
    version: plan.version,
    channel: plan.channel,
    fallback: Boolean(plan.fallback),
    format: plan.format,
    source: plan.source,
    asset: plan.asset.name,
    urls: plan.candidates.slice(0, 4),
  };
}

async function installedVersion() {
  const { status } = await collectStatus({ allowScan: false });
  return status.install?.version ?? null;
}

async function commandInstall(flags) {
  const platform = detectPlatform();
  if (!platform.supported) {
    jsonOutput({
      ok: false,
      error: platform.reason,
      nextAction: "unsupported",
    });
    process.exitCode = 1;
    return;
  }
  if (platform.os === "win32" && platform.arch === "arm64") {
    log("Windows ARM64 将使用 x64 MSI（系统模拟运行）。");
  }

  const channel = flags.channel === "canary" ? "canary" : "stable";
  const dryRun = isTrue(flags["dry-run"]);
  const force = isTrue(flags.force);
  // Explicit opt-in for users who do not want apt/AUR (or for testing the XDG
  // AppImage path on an otherwise Debian/Arch machine).
  const forceAppImage = flags.format === "appimage";

  if (platform.os === "linux") {
    const distro = await detectLinuxDistro();

    if (!forceAppImage && distro.family === "arch") {
      jsonOutput({
        ok: true,
        action: "run-install-command",
        needsUserAction: true,
        distro: "arch",
        distroName: distro.prettyName,
        packageManager: "aur",
        package: "your-szu-life-bin",
        channel: "canary",
        script: archInstallScript(),
        notice: "请把下面的 bash 命令粘贴到终端执行；AUR 包由 CI 跟随 canary。",
        dryRun,
      });
      return;
    }

    const format = !forceAppImage && distro.family === "debian" ? "deb" : "appimage";
    const plan = await resolveInstallPlan({
      platform: "linux",
      arch: platform.arch,
      channel,
      format,
    });

    if (!forceAppImage && distro.family === "debian") {
      const current = await installedVersion();
      if (!force && current && compareVersions(current, plan.version) >= 0) {
        jsonOutput({
          ok: true,
          upToDate: true,
          installedVersion: current,
          version: plan.version,
          channel: plan.channel,
          format: "deb",
        });
        return;
      }
      const command = installCommandFor(plan, distro);
      jsonOutput({
        ok: true,
        ...command,
        dryRun,
        ...(dryRun ? { plan: planSummary(plan) } : {}),
      });
      return;
    }

    if (dryRun) {
      jsonOutput({
        ok: true,
        action: "would-install-appimage",
        dryRun: true,
        plan: planSummary(plan),
        installPathHint: "XDG_DATA_HOME/your-szu-life/your-szu-life.AppImage",
      });
      return;
    }
    if (!isTrue(flags.yes)) {
      jsonOutput({
        ok: false,
        error: "consent-required",
        message: "自动安装 AppImage 需要用户明确同意；确认后加 --yes。",
        plan: planSummary(plan),
      });
      process.exitCode = 2;
      return;
    }
    const current = await installedVersion();
    if (!force && current && compareVersions(current, plan.version) >= 0) {
      jsonOutput({
        ok: true,
        upToDate: true,
        installedVersion: current,
        version: plan.version,
        channel: plan.channel,
        format: "appimage",
      });
      return;
    }
    const result = await installAppImage(plan, { yes: true });
    jsonOutput(result);
    return;
  }

  // Windows
  const plan = await resolveInstallPlan({
    platform: "windows",
    arch: "x64",
    channel,
    format: "msi",
  });
  if (dryRun) {
    jsonOutput({
      ok: true,
      action: "would-install-msi",
      dryRun: true,
      plan: planSummary(plan),
    });
    return;
  }
  if (!isTrue(flags.yes)) {
    jsonOutput({
      ok: false,
      error: "consent-required",
      message: "自动安装 MSI 需要用户明确同意；确认后加 --yes。",
      plan: planSummary(plan),
    });
    process.exitCode = 2;
    return;
  }
  const current = await installedVersion();
  if (!force && current && compareVersions(current, plan.version) >= 0) {
    jsonOutput({
      ok: true,
      upToDate: true,
      installedVersion: current,
      version: plan.version,
      channel: plan.channel,
      format: "msi",
    });
    return;
  }
  const result = await installMsi(plan, { yes: true });
  jsonOutput(result);
}

async function commandLaunch(flags) {
  const result = await launchApp({ visible: isTrue(flags.visible) });
  jsonOutput(result);
  if (!result.ok) process.exitCode = 1;
}

async function commandOpen(target) {
  if (!target) {
    throw new Error("用法: szu-life open <home|settings|mcp-settings|uooc>");
  }
  const result = await openApp(target);
  jsonOutput(result);
  if (!result.ok) process.exitCode = 1;
}

async function appClient(scan = true) {
  const discovery = await readIntegrationFile();
  const found = await discoverMcp({ data: discovery?.value ?? { mcp: {} } }, { allowScan: scan });
  if (!found.reachable || !found.client) return null;
  return found.client;
}

async function commandMcpList() {
  const client = await appClient(true);
  if (!client) {
    jsonOutput({
      ok: false,
      error: "泥深 MCP 尚未就绪，请先运行 szu-life ensure 或完成安装/开启 MCP。",
    });
    process.exitCode = 1;
    return;
  }
  const tools = await client.listTools();
  jsonOutput({
    ok: true,
    tools: tools.map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    })),
  });
}

async function commandMcpCall(tool, jsonArgument) {
  if (!tool) {
    throw new Error("用法: szu-life mcp-call <tool> [json|-|@file]");
  }
  const args = await readJsonArgument(jsonArgument);
  const client = await appClient(true);
  if (!client) {
    jsonOutput({
      ok: false,
      error: "泥深 MCP 尚未就绪，请先运行 szu-life ensure 或完成安装/开启 MCP。",
    });
    process.exitCode = 1;
    return;
  }
  const result = await client.callTool(tool, args);
  jsonOutput(result);
  if (result?.isError) process.exitCode = 1;
}

async function commandMcpUrl() {
  const info = await mcpEndpoint();
  jsonOutput({
    reachable: info.reachable,
    url: info.url,
    port: info.port,
    serverName: info.serverName,
    serverVersion: info.serverVersion,
    source: info.source,
  });
}

export async function main(argv = process.argv.slice(2)) {
  const { positional, flags } = parseArgs(argv);
  const command = positional[0];

  if (!command || command === "help" || command === "--help" || isTrue(flags.help)) {
    process.stdout.write(HELP);
    return;
  }

  if (command === "version") {
    jsonOutput({ ok: true, version: EXPERT_VERSION });
    return;
  }

  if (command === "status") {
    await commandStatus(flags);
    return;
  }

  if (command === "ensure") {
    await commandEnsure(flags);
    return;
  }

  if (command === "install") {
    await commandInstall(flags);
    return;
  }

  if (command === "launch") {
    await commandLaunch(flags);
    return;
  }

  if (command === "open") {
    await commandOpen(positional[1]);
    return;
  }

  if (command === "mcp") {
    await runBridge();
    return;
  }

  if (command === "mcp-list") {
    await commandMcpList();
    return;
  }

  if (command === "mcp-call") {
    await commandMcpCall(positional[1], positional[2]);
    return;
  }

  if (command === "mcp-url") {
    await commandMcpUrl();
    return;
  }

  process.stderr.write(`未知命令: ${command}\n\n${HELP}`);
  process.exitCode = 2;
}

const invokedDirectly =
  process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href;

if (invokedDirectly) {
  main().catch((error) => {
    if (error instanceof ConsentRequiredError) {
      jsonOutput({ ok: false, error: "consent-required", message: error.message });
      process.exitCode = 2;
      return;
    }
    jsonOutput({ ok: false, error: String(error?.message ?? error) });
    log(String(error?.stack ?? error));
    process.exitCode = 1;
  });
}
