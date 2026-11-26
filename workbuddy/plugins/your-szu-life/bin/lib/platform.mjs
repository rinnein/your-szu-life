// Platform, desktop-session, and Linux distribution detection.

import { readOsRelease } from "./paths.mjs";
import { commandExists } from "./util.mjs";

const DEBIAN_IDS = new Set([
  "debian",
  "ubuntu",
  "linuxmint",
  "pop",
  "elementary",
  "zorin",
  "kali",
  "raspbian",
  "neon",
  "deepin",
  "uos",
]);

const ARCH_IDS = new Set([
  "arch",
  "manjaro",
  "endeavouros",
  "garuda",
  "artix",
  "cachyos",
  "arcolinux",
  "rebornos",
]);

// Immutable/atomic distros: package-manager installs are a bad fit, so use the
// per-user AppImage path even when the distro is Arch/Debian-like on paper.
const IMMUTABLE_IDS = new Set([
  "steamos",
  "bazzite",
  "bluefin",
  "aurora",
  "silverblue",
  "kinoite",
  "sericea",
  "onyx",
  "nixos",
  "guix",
  "endless",
  "fedora-silverblue",
  "fedora-kinoite",
]);

export function parseOsRelease(text) {
  const fields = {};
  for (const rawLine of String(text ?? "").split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    const equals = line.indexOf("=");
    if (equals <= 0) continue;
    const key = line.slice(0, equals).trim();
    let value = line.slice(equals + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    fields[key] = value;
  }
  return fields;
}

/**
 * Pure classifier so tests can feed fixtures without touching /etc/os-release.
 * Returns "debian" | "arch" | "other".
 */
export function detectLinuxFamily(osReleaseText, commands = {}) {
  const fields = parseOsRelease(osReleaseText);
  const id = (fields.ID || "").toLowerCase();
  const idLike = (fields.ID_LIKE || "").toLowerCase().split(/\s+/).filter(Boolean);
  const ids = [id, ...idLike].filter(Boolean);

  if (ids.some((entry) => IMMUTABLE_IDS.has(entry))) {
    return "other";
  }
  if (ids.some((entry) => DEBIAN_IDS.has(entry))) {
    return "debian";
  }
  if (ids.some((entry) => ARCH_IDS.has(entry))) {
    return "arch";
  }
  if (commands.hasApt) return "debian";
  if (commands.hasPacman) return "arch";
  return "other";
}

export function detectPlatform() {
  const os = process.platform;
  const arch = process.arch;

  let desktop = false;
  if (os === "linux") {
    desktop = Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
  } else if (os === "win32") {
    desktop = Boolean(process.env.USERPROFILE && process.env.SESSIONNAME);
  }

  let supported = false;
  let reason = "";
  if (os === "darwin") {
    reason = "泥深目前只提供 Windows 与 Linux 桌面版，暂不支持 macOS。";
  } else if (os === "linux" && arch !== "x64") {
    reason = `Linux 目前只提供 x64 安装包，当前架构是 ${arch}。`;
  } else if (os === "win32" && arch !== "x64" && arch !== "arm64") {
    reason = `Windows 目前只提供 x64 安装包，当前架构是 ${arch}。`;
  } else if (os !== "linux" && os !== "win32") {
    reason = `暂不支持当前操作系统：${os}。`;
  } else if (!desktop) {
    reason =
      "当前环境没有桌面会话（未检测到 DISPLAY/WAYLAND_DISPLAY 或交互式 Windows 会话），不能安装桌面应用。请在本机桌面版 WorkBuddy 中使用。";
  } else {
    supported = true;
  }

  return {
    os,
    arch,
    desktop,
    supported,
    reason,
    archNote:
      os === "win32" && arch === "arm64" ? "Windows ARM64 将通过 x64 模拟运行安装包。" : undefined,
  };
}

export async function detectLinuxDistro() {
  const osReleaseText = await readOsRelease();
  const fields = parseOsRelease(osReleaseText);
  const hasApt = await commandExists("apt-get");
  const hasPacman = await commandExists("pacman");
  return {
    id: fields.ID || "",
    idLike: fields.ID_LIKE || "",
    prettyName: fields.PRETTY_NAME || fields.NAME || fields.ID || "Linux",
    family: detectLinuxFamily(osReleaseText, { hasApt, hasPacman }),
    hasApt,
    hasPacman,
  };
}
