// Process liveness and app-process discovery.

import { APP_BINARY } from "./constants.mjs";
import { run, sleep } from "./util.mjs";

export function isPidAlive(pid) {
  const value = Number(pid);
  if (!Number.isInteger(value) || value <= 0) return false;
  try {
    process.kill(value, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

async function linuxProcessList() {
  const result = await run("ps", ["-eo", "pid=,comm=,args="], {
    timeoutMs: 5_000,
  });
  if (result.code !== 0) return [];
  return result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const match = line.match(/^(\d+)\s+(\S+)\s*(.*)$/);
      if (!match) return null;
      return { pid: Number(match[1]), comm: match[2], args: match[3] ?? "" };
    })
    .filter(Boolean);
}

async function windowsProcessList() {
  const result = await run(
    "tasklist",
    ["/FI", `IMAGENAME eq ${APP_BINARY}.exe`, "/FO", "CSV", "/NH"],
    { timeoutMs: 8_000 },
  );
  if (result.code !== 0) return [];
  return result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.startsWith('"'))
    .map((line) => {
      const columns = line
        .slice(1, -1)
        .split('","')
        .map((column) => column.replace(/""/g, '"'));
      return columns[1] ? { pid: Number(columns[1]), comm: columns[0], args: "" } : null;
    })
    .filter((entry) => entry && Number.isInteger(entry.pid));
}

export async function listAppProcesses() {
  if (process.platform === "win32") {
    return windowsProcessList();
  }
  if (process.platform === "linux") {
    const processes = await linuxProcessList();
    // Match the executable name only. Checking argv would also match this
    // bridge because its plugin directory is literally named your-szu-life.
    return processes.filter((entry) => entry.pid !== process.pid && entry.comm === APP_BINARY);
  }
  return [];
}

export async function findAppProcess() {
  const processes = await listAppProcesses();
  return processes[0] ?? null;
}

export async function waitFor(predicate, options = {}) {
  const { timeoutMs = 20_000, intervalMs = 500 } = options;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await predicate();
    if (value) return value;
    await sleep(intervalMs);
  }
  return null;
}
