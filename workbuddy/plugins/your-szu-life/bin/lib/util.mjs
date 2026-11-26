// Small dependency-free helpers shared by the bridge, installer, and CLI.

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  access,
  chmod,
  copyFile,
  mkdir,
  open,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import os from "node:os";
import path from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

import { TMP_PREFIX } from "./constants.mjs";

export function log(...args) {
  console.error("[szu-life]", ...args);
}

export function nowMs() {
  return Date.now();
}

export function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function jsonOutput(value) {
  process.stdout.write(`${JSON.stringify(value, null, 2)}\n`);
}

export class ConsentRequiredError extends Error {
  constructor(message = "需要用户明确同意后才能执行安装") {
    super(message);
    this.name = "ConsentRequiredError";
  }
}

/**
 * Spawn a process without a shell. Returns captured stdout/stderr and exit code.
 * `env` can be supplied to add variables without replacing the inherited environment.
 */
export function run(command, args = [], options = {}) {
  const { timeoutMs = 30_000, cwd, env, input, allowFailure = true, stdio = "pipe" } = options;

  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(command, args, {
        cwd,
        env: env ? { ...process.env, ...env } : process.env,
        windowsHide: true,
        shell: false,
        stdio,
      });
    } catch (error) {
      resolve({ code: -1, stdout: "", stderr: String(error) });
      return;
    }

    let stdout = "";
    let stderr = "";
    let settled = false;

    const finish = (code, extra = "") => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      resolve({ code, stdout, stderr: stderr + extra });
      if (!allowFailure && code !== 0) {
        // The caller will inspect the result; this branch only documents intent.
      }
    };

    const timer =
      timeoutMs > 0
        ? setTimeout(() => {
            try {
              child.kill("SIGKILL");
            } catch {
              // ignore
            }
            finish(-1, "\n[timeout]");
          }, timeoutMs)
        : null;

    if (child.stdout) child.stdout.on("data", (chunk) => (stdout += chunk));
    if (child.stderr) child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", (error) => {
      finish(-1, String(error));
    });
    child.on("close", (code) => finish(code ?? -1));

    if (input !== undefined && child.stdin) {
      child.stdin.end(input);
    }
  });
}

export async function pathExists(target) {
  try {
    await access(target, fsConstants.F_OK);
    return true;
  } catch {
    return false;
  }
}

export async function isExecutable(target) {
  try {
    await access(target, fsConstants.X_OK);
    return true;
  } catch {
    return false;
  }
}

export function splitPathEnv() {
  return (process.env.PATH ?? "")
    .split(path.delimiter)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

export async function findInPath(name) {
  const extensions = process.platform === "win32" ? [".exe", ".cmd", ".bat", ".com", ""] : [""];
  for (const directory of splitPathEnv()) {
    for (const extension of extensions) {
      const candidate = path.join(directory, `${name}${extension}`);
      if (await isExecutable(candidate)) return candidate;
    }
  }
  return null;
}

export async function commandExists(name) {
  return (await findInPath(name)) !== null;
}

export async function readJsonFile(target) {
  try {
    const text = await readFile(target, "utf8");
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export async function writeFileAtomic(target, data, mode = 0o600) {
  const directory = path.dirname(target);
  await mkdir(directory, { recursive: true });
  const tmp = path.join(directory, `.${path.basename(target)}.${process.pid}.${Date.now()}.tmp`);
  await writeFile(tmp, data, { mode });
  if (process.platform === "win32") {
    await rm(target, { force: true });
  }
  try {
    await rename(tmp, target);
  } catch (error) {
    await rm(tmp, { force: true });
    throw error;
  }
}

export async function writeJsonAtomic(target, value, mode = 0o600) {
  await writeFileAtomic(target, `${JSON.stringify(value, null, 2)}\n`, mode);
}

export function shellSingleQuote(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

export function powershellSingleQuote(value) {
  return `'${String(value).replace(/'/g, "''")}'`;
}

export function desktopExecQuote(value) {
  return `"${String(value).replace(/"/g, '\\"')}"`;
}

export async function runPowerShell(script, options = {}) {
  if (process.platform !== "win32") {
    throw new Error("PowerShell 仅在 Windows 上可用");
  }
  const directory = await mkdtemp();
  const file = path.join(directory, "script.ps1");
  // BOM keeps Windows PowerShell 5.1 from misreading non-ASCII paths.
  await writeFile(file, `\uFEFF${script}`, "utf8");
  try {
    return await run(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", file],
      options,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

export async function mkdtemp() {
  const directory = path.join(
    os.tmpdir(),
    `${TMP_PREFIX}${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}`,
  );
  await mkdir(directory, { recursive: true, mode: 0o700 });
  return directory;
}

export async function fileSize(target) {
  try {
    return (await stat(target)).size;
  } catch {
    return 0;
  }
}

export async function sha256File(target) {
  const hash = createHash("sha256");
  await pipeline(createReadStream(target), hash);
  return hash.digest("hex");
}

export async function hasMagic(target, bytes) {
  try {
    const handle = await open(target, "r");
    try {
      const buffer = Buffer.alloc(bytes.length);
      const { bytesRead } = await handle.read(buffer, 0, bytes.length, 0);
      return bytesRead === bytes.length && buffer.equals(Buffer.from(bytes));
    } finally {
      await handle.close();
    }
  } catch {
    return false;
  }
}

export function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

/**
 * Fetch text with the platform-native HTTP stack first (so proxy settings are
 * honoured), then fall back to Node's fetch. JSON APIs should pass acceptJson.
 */
export async function fetchText(url, options = {}) {
  const { timeoutMs = 20_000, acceptJson = false, headers = {}, preferNative = true } = options;
  const accept = acceptJson ? "application/json" : "*/*";

  if (preferNative && process.platform !== "win32" && (await commandExists("curl"))) {
    const result = await run(
      "curl",
      [
        "-fsSL",
        "--max-time",
        String(Math.max(1, Math.ceil(timeoutMs / 1000))),
        "-H",
        `Accept: ${accept}`,
        ...Object.entries(headers).flatMap(([key, value]) => ["-H", `${key}: ${value}`]),
        url,
      ],
      { timeoutMs: timeoutMs + 3_000 },
    );
    if (result.code === 0) return result.stdout;
  }

  if (preferNative && process.platform === "win32") {
    const headerBlock = Object.entries({ Accept: accept, ...headers })
      .map(([key, value]) => `${powershellSingleQuote(key)} = ${powershellSingleQuote(value)}`)
      .join("; ");
    const script = `
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$headers = @{ ${headerBlock} }
$response = Invoke-WebRequest -UseBasicParsing -Uri ${powershellSingleQuote(url)} -Headers $headers -TimeoutSec ${Math.max(1, Math.ceil(timeoutMs / 1000))}
[Console]::Out.Write($response.Content)
`;
    const result = await runPowerShell(script, { timeoutMs: timeoutMs + 3_000 });
    if (result.code === 0) return result.stdout;
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, {
      headers: { Accept: accept, "User-Agent": "your-szu-life-workbuddy", ...headers },
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} ${response.statusText}`);
    }
    return await response.text();
  } finally {
    clearTimeout(timer);
  }
}

export async function fetchJson(url, options = {}) {
  const text = await fetchText(url, { ...options, acceptJson: true });
  return JSON.parse(text);
}

/**
 * Download a file using curl/wget/PowerShell (proxy-aware) with a Node fetch
 * fallback. The parent directory must already exist.
 */
export async function downloadFile(url, destination, options = {}) {
  const { timeoutMs = 30 * 60_000, onStart } = options;
  if (onStart) onStart(url);

  if (process.platform !== "win32" && (await commandExists("curl"))) {
    const result = await run(
      "curl",
      [
        "-fL",
        "--retry",
        "3",
        "--retry-delay",
        "2",
        "--connect-timeout",
        "20",
        "--max-time",
        String(Math.max(1, Math.ceil(timeoutMs / 1000))),
        "-o",
        destination,
        url,
      ],
      { timeoutMs: timeoutMs + 30_000 },
    );
    if (result.code === 0) return "curl";
  }

  if (process.platform !== "win32" && (await commandExists("wget"))) {
    const result = await run("wget", ["-q", "-O", destination, url], {
      timeoutMs: timeoutMs + 30_000,
    });
    if (result.code === 0) return "wget";
  }

  if (process.platform === "win32") {
    const script = `
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -UseBasicParsing -Uri ${powershellSingleQuote(url)} -OutFile ${powershellSingleQuote(destination)} -TimeoutSec ${Math.max(1, Math.ceil(timeoutMs / 1000))}
`;
    const result = await runPowerShell(script, { timeoutMs: timeoutMs + 30_000 });
    if (result.code === 0) return "powershell";
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, {
      headers: { "User-Agent": "your-szu-life-workbuddy" },
      signal: controller.signal,
    });
    if (!response.ok || !response.body) {
      throw new Error(`HTTP ${response.status} ${response.statusText}`);
    }
    await pipeline(
      Readable.fromWeb(response.body),
      (await import("node:fs")).createWriteStream(destination),
    );
    return "node";
  } finally {
    clearTimeout(timer);
  }
}

export async function copyFileWithMode(source, destination, mode = 0o644) {
  await mkdir(path.dirname(destination), { recursive: true });
  await copyFile(source, destination);
  await chmod(destination, mode);
}
