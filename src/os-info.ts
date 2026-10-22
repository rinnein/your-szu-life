import {
  arch as tauriArch,
  platform as tauriPlatform,
  type as tauriType,
  version as tauriVersion,
} from "@tauri-apps/plugin-os";

function isTauriEnvironment() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getPlatform(): Promise<string> {
  if (isTauriEnvironment()) {
    try {
      return await tauriPlatform();
    } catch {
      // Fall through
    }
  }
  if (typeof navigator !== "undefined") {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes("android")) return "android";
    if (ua.includes("linux")) return "linux";
    if (ua.includes("mac")) return "macos";
    if (ua.includes("win")) return "windows";
  }
  return "unknown";
}

export async function getOsType(): Promise<string> {
  if (isTauriEnvironment()) {
    try {
      return await tauriType();
    } catch {
      // Fall through
    }
  }
  return getPlatform();
}

export async function getOsVersion(): Promise<string> {
  if (isTauriEnvironment()) {
    try {
      return await tauriVersion();
    } catch {
      // Fall through
    }
  }
  return "";
}

export async function getArch(): Promise<string> {
  if (isTauriEnvironment()) {
    try {
      return await tauriArch();
    } catch {
      // Fall through
    }
  }
  return "unknown";
}

export async function isAndroidPlatform(): Promise<boolean> {
  const p = await getPlatform();
  return p === "android";
}

export async function isWindowsPlatform(): Promise<boolean> {
  const p = await getPlatform();
  return p === "windows";
}

export async function isLinuxPlatform(): Promise<boolean> {
  const p = await getPlatform();
  return p === "linux";
}
