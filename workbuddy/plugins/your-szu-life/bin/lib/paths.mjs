// XDG/Windows path resolution and discovery-file helpers.

import os from "node:os";
import path from "node:path";
import { readFile } from "node:fs/promises";

import { APP_BINARY, APP_ID } from "./constants.mjs";
import { findInPath, pathExists, readJsonFile } from "./util.mjs";

export function homeDir() {
  return os.homedir();
}

export function dataHome() {
  if (process.platform === "win32") {
    return process.env.APPDATA || path.join(homeDir(), "AppData", "Roaming");
  }
  return process.env.XDG_DATA_HOME || path.join(homeDir(), ".local", "share");
}

export function localDataHome() {
  if (process.platform === "win32") {
    return process.env.LOCALAPPDATA || path.join(homeDir(), "AppData", "Local");
  }
  return dataHome();
}

export function binHome() {
  return process.env.XDG_BIN_HOME || path.join(homeDir(), ".local", "bin");
}

export function appDataDir() {
  return path.join(dataHome(), APP_ID);
}

export function appDataDirCandidates() {
  const candidates = [appDataDir()];
  if (process.platform === "win32") {
    candidates.push(path.join(localDataHome(), APP_ID));
  }
  return [...new Set(candidates)];
}

export function integrationPaths() {
  return appDataDirCandidates().map((directory) => path.join(directory, "integration.json"));
}

export function installMetaPaths() {
  return appDataDirCandidates().map((directory) => path.join(directory, "install.json"));
}

export function appImagePath() {
  return path.join(dataHome(), APP_BINARY, `${APP_BINARY}.AppImage`);
}

export function launcherPath() {
  const preferred = path.join(binHome(), APP_BINARY);
  return preferred;
}

export function fallbackLauncherPath() {
  return path.join(binHome(), `${APP_BINARY}-appimage`);
}

export function desktopFilePath() {
  return path.join(dataHome(), "applications", `${APP_BINARY}.desktop`);
}

export function iconPath() {
  return path.join(dataHome(), "icons", "hicolor", "512x512", "apps", `${APP_BINARY}.png`);
}

export async function readFirstExistingJson(paths) {
  for (const candidate of paths) {
    const value = await readJsonFile(candidate);
    if (value) return { path: candidate, value };
  }
  return null;
}

export async function readIntegrationFile() {
  return readFirstExistingJson(integrationPaths());
}

export async function readInstallMeta() {
  return readFirstExistingJson(installMetaPaths());
}

export async function windowsBinaryCandidates(installPath) {
  const candidates = [];
  if (installPath) candidates.push(installPath);
  const programFiles = process.env.ProgramFiles;
  const programFilesX86 = process.env["ProgramFiles(x86)"];
  const localAppData = localDataHome();
  for (const root of [programFiles, programFilesX86]) {
    if (root) {
      candidates.push(path.join(root, APP_BINARY, `${APP_BINARY}.exe`));
      candidates.push(path.join(root, APP_BINARY, "your-szu-life.exe"));
    }
  }
  candidates.push(
    path.join(localAppData, "Programs", APP_BINARY, `${APP_BINARY}.exe`),
    path.join(localAppData, APP_BINARY, `${APP_BINARY}.exe`),
  );
  return candidates;
}

export async function findAppBinary(installMeta) {
  const candidates = [];
  if (installMeta?.launcher) candidates.push(installMeta.launcher);
  if (installMeta?.path) candidates.push(installMeta.path);

  if (process.platform === "win32") {
    candidates.push(...(await windowsBinaryCandidates(installMeta?.path)));
  } else {
    candidates.push(
      path.join(launcherPath()),
      path.join(fallbackLauncherPath()),
      appImagePath(),
      `/usr/bin/${APP_BINARY}`,
      `/usr/local/bin/${APP_BINARY}`,
      `/opt/${APP_BINARY}/${APP_BINARY}`,
    );
  }

  const pathHit = await findInPath(APP_BINARY);
  if (pathHit) candidates.push(pathHit);

  const seen = new Set();
  for (const candidate of candidates) {
    if (!candidate || seen.has(candidate)) continue;
    seen.add(candidate);
    if (await pathExists(candidate)) return candidate;
  }
  return null;
}

export async function readOsRelease() {
  for (const candidate of ["/etc/os-release", "/usr/lib/os-release"]) {
    try {
      return await readFile(candidate, "utf8");
    } catch {
      // try the next location
    }
  }
  return "";
}
