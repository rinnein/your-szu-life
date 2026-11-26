// Linux installation strategy:
//   Debian family -> emit a deb install bash block for the user to run.
//   Arch family   -> emit an AUR install bash block for the user to run.
//   Everything else -> install the AppImage into XDG user directories.
//
// Debian/Arch are deliberately not executed by the agent: apt/makepkg need
// sudo and the user should see exactly what will run.

import { fileURLToPath } from "node:url";
import { readFile, chmod, mkdir, rename, rm } from "node:fs/promises";
import path from "node:path";

import {
  APP_NAME,
  AUR_PACKAGE,
  INSTALL_MARKER,
  INSTALL_META_SCHEMA_VERSION,
} from "./constants.mjs";
import {
  appDataDir,
  appImagePath,
  desktopFilePath,
  iconPath,
  launcherPath,
  fallbackLauncherPath,
} from "./paths.mjs";
import {
  ConsentRequiredError,
  copyFileWithMode,
  log,
  pathExists,
  run,
  shellSingleQuote,
  writeFileAtomic,
  writeJsonAtomic,
} from "./util.mjs";
import { downloadVerified } from "./install-common.mjs";

export function debianInstallScript(plan) {
  const urls = plan.candidates ?? [];
  if (!urls.length) throw new Error("没有可用的 deb 下载地址");
  const primary = shellSingleQuote(urls[0]);
  const fallback = urls[1] ? shellSingleQuote(urls[1]) : null;
  const header = `# ${APP_NAME} ${plan.version}（${plan.channel}${plan.fallback ? "，正式版暂无 Linux 包，已回退 canary" : ""}）`;
  const fallbackLine = fallback
    ? `download ${primary} "$tmp_dir/your-szu-life.deb" || download ${fallback} "$tmp_dir/your-szu-life.deb"`
    : `download ${primary} "$tmp_dir/your-szu-life.deb"`;
  const checksumLine = plan.asset?.sha256
    ? `echo "${plan.asset.sha256}  $tmp_dir/your-szu-life.deb" | sha256sum -c -`
    : "";

  return `${header}
set -e
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
download() {
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --connect-timeout 20 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    echo "需要 curl 或 wget 才能下载 deb" >&2
    return 1
  fi
}
${fallbackLine}
${checksumLine}
if command -v apt >/dev/null 2>&1; then
  sudo apt install -y "$tmp_dir/your-szu-life.deb"
else
  sudo apt-get install -y "$tmp_dir/your-szu-life.deb"
fi`;
}

export function archInstallScript() {
  return `# 泥深使用 AUR 包 ${AUR_PACKAGE}（provides your-szu-life；该包由 CI 跟随 canary）
if command -v yay >/dev/null 2>&1; then
  yay -S --needed ${AUR_PACKAGE}
elif command -v paru >/dev/null 2>&1; then
  paru -S --needed ${AUR_PACKAGE}
else
  tmp_dir="$(mktemp -d)"
  git clone https://aur.archlinux.org/${AUR_PACKAGE}.git "$tmp_dir/${AUR_PACKAGE}"
  (cd "$tmp_dir/${AUR_PACKAGE}" && makepkg -si)
fi`;
}

export function installCommandFor(plan, distro) {
  if (distro.family === "debian") {
    return {
      action: "run-install-command",
      needsUserAction: true,
      distro: "debian",
      distroName: distro.prettyName,
      packageManager: "apt",
      version: plan.version,
      channel: plan.channel,
      fallback: plan.fallback,
      urls: plan.candidates,
      script: debianInstallScript(plan),
      notice:
        "请把下面的 bash 命令粘贴到终端执行。它会下载 deb 并用 apt 安装，需要输入 sudo 密码。",
    };
  }
  if (distro.family === "arch") {
    return {
      action: "run-install-command",
      needsUserAction: true,
      distro: "arch",
      distroName: distro.prettyName,
      packageManager: "aur",
      package: AUR_PACKAGE,
      version: plan.version,
      channel: plan.channel,
      script: archInstallScript(),
      notice: `请把下面的 bash 命令粘贴到终端执行。AUR 包 ${AUR_PACKAGE} 由 CI 跟随 canary，可能不是最新正式版。`,
    };
  }
  throw new Error("installCommandFor 只处理 Debian/Arch");
}

async function fuseAvailable() {
  const ldconfig = await run("ldconfig", ["-p"], { timeoutMs: 5_000 });
  if (ldconfig.code === 0 && /libfuse\.so\.2/i.test(ldconfig.stdout)) {
    return true;
  }
  for (const candidate of [
    "/usr/lib/x86_64-linux-gnu/libfuse.so.2",
    "/usr/lib64/libfuse.so.2",
    "/usr/lib/libfuse.so.2",
  ]) {
    if (await pathExists(candidate)) return true;
  }
  return false;
}

export async function installAppImage(plan, options = {}) {
  const { yes = false } = options;
  if (!yes) throw new ConsentRequiredError();

  const target = appImagePath();
  const targetDirectory = path.dirname(target);
  await mkdir(targetDirectory, { recursive: true, mode: 0o755 });

  const temporary = path.join(targetDirectory, `.your-szu-life.AppImage.${process.pid}.tmp`);
  await rm(temporary, { force: true }).catch(() => {});

  log(`安装 AppImage 到 ${target}`);
  const downloaded = await downloadVerified(plan.candidates, temporary, {
    kind: "elf",
    sha256: plan.asset.sha256,
    minSize: 5 * 1024 * 1024,
  });
  await chmod(temporary, 0o755);
  await rename(temporary, target);

  const extractAndRun = !(await fuseAvailable());
  let launcher = launcherPath();
  const existingLauncher = await readFile(launcher, "utf8").catch(() => null);
  if (existingLauncher !== null && !existingLauncher.includes(INSTALL_MARKER)) {
    launcher = fallbackLauncherPath();
  }
  const existingFallback = await readFile(launcher, "utf8").catch(() => null);
  if (existingFallback !== null && !existingFallback.includes(INSTALL_MARKER)) {
    throw new Error(`启动器路径 ${launcher} 已被其它程序占用，请移除后重试`);
  }

  const quote = target.replace(/"/g, '\\"');
  const launcherScript = `#!/bin/sh
${INSTALL_MARKER}
${extractAndRun ? "export APPIMAGE_EXTRACT_AND_RUN=1" : ":"}
exec "${quote}" "$@"
`;
  await writeFileAtomic(launcher, launcherScript, 0o755);
  await chmod(launcher, 0o755);

  const desktop = desktopFilePath();
  const desktopContent = `${INSTALL_MARKER}
[Desktop Entry]
Type=Application
Name=${APP_NAME}
Name[zh_CN]=${APP_NAME}
Comment=你的SZU助手
Exec="${launcher.replace(/"/g, '\\"')}" %u
Icon=your-szu-life
Terminal=false
Categories=Utility;Education;
StartupWMClass=your-szu-life
MimeType=x-scheme-handler/your-szu-life;
`;
  await writeFileAtomic(desktop, desktopContent, 0o644);

  const avatar = fileURLToPath(new URL("../../avatars/your-szu-life.png", import.meta.url));
  try {
    await copyFileWithMode(avatar, iconPath(), 0o644);
  } catch (error) {
    log(`复制图标失败（不影响使用）: ${String(error)}`);
  }

  await run("xdg-mime", ["default", "your-szu-life.desktop", "x-scheme-handler/your-szu-life"], {
    timeoutMs: 5_000,
  });
  await run("update-desktop-database", [path.dirname(desktop)], {
    timeoutMs: 5_000,
  });

  const installedAt = new Date().toISOString();
  await writeJsonAtomic(path.join(appDataDir(), "install.json"), {
    schemaVersion: INSTALL_META_SCHEMA_VERSION,
    version: plan.version,
    channel: plan.channel,
    fallback: Boolean(plan.fallback),
    format: "appimage",
    source: plan.source,
    path: target,
    launcher,
    installedAt,
    extractAndRun,
  });

  return {
    ok: true,
    action: "installed",
    version: plan.version,
    channel: plan.channel,
    fallback: Boolean(plan.fallback),
    format: "appimage",
    source: plan.source,
    path: target,
    launcher,
    installedAt,
    extractAndRun,
    downloadedFrom: downloaded.url,
  };
}
