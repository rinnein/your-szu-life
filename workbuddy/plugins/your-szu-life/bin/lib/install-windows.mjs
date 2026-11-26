// Windows installation: download the MSI, elevate once for msiexec, then write
// the per-user URL protocol registration that the installer does not create.

import path from "node:path";
import { mkdtemp, rm } from "node:fs/promises";

import { INSTALL_META_SCHEMA_VERSION } from "./constants.mjs";
import { appDataDir, windowsBinaryCandidates } from "./paths.mjs";
import {
  ConsentRequiredError,
  log,
  pathExists,
  powershellSingleQuote,
  runPowerShell,
  sleep,
  writeJsonAtomic,
} from "./util.mjs";
import { downloadVerified } from "./install-common.mjs";

async function findInstalledBinary(hint) {
  const candidates = await windowsBinaryCandidates(hint);
  for (const candidate of candidates) {
    if (candidate && (await pathExists(candidate))) return candidate;
  }
  return null;
}

async function waitForInstalledBinary(hint, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const found = await findInstalledBinary(hint);
    if (found) return found;
    await sleep(1_000);
  }
  return findInstalledBinary(hint);
}

export async function registerUrlProtocol(exePath) {
  const script = `
$ErrorActionPreference = 'Stop'
$exe = ${powershellSingleQuote(exePath)}
$key = 'HKCU:\\Software\\Classes\\your-szu-life'
New-Item -Path $key -Force | Out-Null
Set-Item -Path $key -Value 'URL:your-szu-life protocol'
New-ItemProperty -Path $key -Name 'URL Protocol' -Value '' -PropertyType String -Force | Out-Null
New-Item -Path "$key\\DefaultIcon" -Force | Out-Null
Set-Item -Path "$key\\DefaultIcon" -Value ('"' + $exe + '",0')
New-Item -Path "$key\\shell\\open\\command" -Force | Out-Null
Set-Item -Path "$key\\shell\\open\\command" -Value ('"' + $exe + '" "%1"')
`;
  const result = await runPowerShell(script, { timeoutMs: 15_000 });
  if (result.code !== 0) {
    log(`写入 HKCU URL 协议失败（应用启动后还会再注册一次）: ${result.stderr.trim()}`);
    return false;
  }
  return true;
}

export async function installMsi(plan, options = {}) {
  const { yes = false } = options;
  if (!yes) throw new ConsentRequiredError();

  const directory = await mkdtemp();
  const msi = path.join(directory, "your-szu-life.msi");
  try {
    log(`下载 MSI: ${plan.version}`);
    const downloaded = await downloadVerified(plan.candidates, msi, {
      kind: "ole",
      sha256: plan.asset.sha256,
      minSize: 5 * 1024 * 1024,
    });

    const script = `
$ErrorActionPreference = 'Stop'
try {
  $process = Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/i', $env:SZU_MSI, '/qn', '/norestart') -Verb RunAs -Wait -PassThru
  Write-Output $process.ExitCode
} catch {
  Write-Error $_.Exception.Message
  exit 1
}
`;
    log("请求管理员权限并静默安装 MSI（会出现 UAC 提示）");
    const result = await runPowerShell(script, {
      timeoutMs: 10 * 60_000,
      env: { SZU_MSI: msi },
    });
    if (result.code !== 0) {
      throw new Error(
        `MSI 安装失败（退出码 ${result.code}）：${result.stderr.trim() || result.stdout.trim()}`,
      );
    }
    const exitCode = Number.parseInt(result.stdout.trim().split(/\s+/).pop() ?? "", 10);
    if (!(exitCode === 0 || exitCode === 3010)) {
      throw new Error(`msiexec 返回退出码 ${Number.isNaN(exitCode) ? "未知" : exitCode}`);
    }

    const exe = await waitForInstalledBinary();
    if (!exe) {
      throw new Error(
        "MSI 安装完成，但没有在 Program Files / LocalAppData 找到 your-szu-life.exe。请打开开始菜单确认安装位置。",
      );
    }

    await registerUrlProtocol(exe);

    const installedAt = new Date().toISOString();
    await writeJsonAtomic(path.join(appDataDir(), "install.json"), {
      schemaVersion: INSTALL_META_SCHEMA_VERSION,
      version: plan.version,
      channel: plan.channel,
      fallback: Boolean(plan.fallback),
      format: "msi",
      source: plan.source,
      path: exe,
      installedAt,
    });

    return {
      ok: true,
      action: "installed",
      version: plan.version,
      channel: plan.channel,
      fallback: Boolean(plan.fallback),
      format: "msi",
      source: plan.source,
      path: exe,
      installedAt,
      downloadedFrom: downloaded.url,
      rebootRequired: exitCode === 3010,
    };
  } finally {
    await rm(directory, { recursive: true, force: true }).catch(() => {});
  }
}
