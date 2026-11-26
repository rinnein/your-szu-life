// Read the app's integration contract, discover the dynamic MCP endpoint, and
// compute the next onboarding step for the expert.

import { APP_NAME, DEEP_LINKS, MCP_PORT_RANGE } from "./constants.mjs";
import { findAppBinary, readInstallMeta, readIntegrationFile } from "./paths.mjs";
import { detectLinuxDistro, detectPlatform } from "./platform.mjs";
import { findAppProcess, isPidAlive } from "./processes.mjs";
import { probeMcp, probeTcp } from "./mcp-client.mjs";
import { run } from "./util.mjs";

function textOfToolResult(result) {
  const content = result?.content;
  if (!Array.isArray(content)) return null;
  const texts = content
    .filter((entry) => entry && entry.type === "text" && typeof entry.text === "string")
    .map((entry) => entry.text);
  return texts.length ? texts.join("\n") : null;
}

function safeJson(text) {
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function authInfoFromStateValue(value) {
  if (!value || typeof value !== "object") return null;
  const key = Object.keys(value)[0];
  const labels = {
    Checking: "checking",
    checking: "checking",
    Unauthenticated: "unauthenticated",
    unauthenticated: "unauthenticated",
    Authenticating: "authenticating",
    authenticating: "authenticating",
    AwaitingProfile: "awaitingProfile",
    awaitingProfile: "awaitingProfile",
    Authenticated: "authenticated",
    authenticated: "authenticated",
    CredentialCaptureFailed: "credentialCaptureFailed",
    credentialCaptureFailed: "credentialCaptureFailed",
    SessionCaptureFailed: "sessionCaptureFailed",
    sessionCaptureFailed: "sessionCaptureFailed",
    CredentialStorageFailed: "credentialStorageFailed",
    credentialStorageFailed: "credentialStorageFailed",
    AccountMismatch: "accountMismatch",
    accountMismatch: "accountMismatch",
    SessionExpired: "sessionExpired",
    sessionExpired: "sessionExpired",
    WebViewUpdateRequired: "webViewUpdateRequired",
    webViewUpdateRequired: "webViewUpdateRequired",
  };
  if (!key || !labels[key]) return null;
  return {
    state: labels[key],
    loggedIn: key === "Authenticated" || key === "authenticated",
  };
}

function onboardingInfoFromStateValue(value) {
  if (!value || typeof value !== "object") return null;
  if (typeof value.needsSetup !== "boolean") return null;
  return {
    available: true,
    needsSetup: value.needsSetup,
    currentStep: value.currentStep ?? null,
    steps: Array.isArray(value.steps) ? value.steps : [],
  };
}

async function remoteStatusFromMcp(client) {
  const result = { onboarding: null, auth: null, uooc: null };

  try {
    const response = await client.callTool("integration_get_status", {});
    const payload = safeJson(textOfToolResult(response));
    if (payload) {
      if (payload.onboarding) result.onboarding = payload.onboarding;
      if (payload.auth) result.auth = payload.auth;
      if (payload.uooc) result.uooc = payload.uooc;
      return result;
    }
  } catch {
    // Older app versions do not have integration_get_status.
  }

  try {
    const response = await client.callTool("onboarding_get_status", {});
    result.onboarding = onboardingInfoFromStateValue(safeJson(textOfToolResult(response)));
  } catch {
    // ignore
  }

  try {
    const response = await client.callTool("auth_get_status", {});
    result.auth = authInfoFromStateValue(safeJson(textOfToolResult(response)));
  } catch {
    // ignore
  }

  try {
    const response = await client.readResource("your-szu-life://data/uooc/courses");
    const content = response?.contents?.[0]?.text;
    const snapshot = safeJson(content);
    if (snapshot?.session) {
      result.uooc = {
        session: snapshot.session,
        loggedIn: snapshot.session === "ready",
      };
    }
  } catch {
    // ignore
  }

  return result;
}

async function linuxPackageVersion(binaryPath, family) {
  if (process.platform !== "linux" || !binaryPath) return null;
  if (family === "debian") {
    const owner = await run("dpkg-query", ["-S", binaryPath], { timeoutMs: 5_000 });
    if (owner.code === 0) {
      const match = owner.stdout.match(/^([^:]+):/);
      if (match) {
        const version = await run("dpkg-query", ["-W", "-f=${Version}", match[1]], {
          timeoutMs: 5_000,
        });
        if (version.code === 0 && version.stdout.trim()) {
          return { manager: "deb", package: match[1], version: version.stdout.trim() };
        }
      }
    }
  }
  if (family === "arch") {
    const owner = await run("pacman", ["-Qo", binaryPath], { timeoutMs: 5_000 });
    if (owner.code === 0) {
      const match = owner.stdout.match(/owned by (\S+) (\S+)/);
      if (match) {
        return { manager: "aur", package: match[1], version: match[2] };
      }
    }
  }
  return null;
}

async function scanForMcp(preferredPort, options = {}) {
  const { concurrency = 25, timeoutMs = 180 } = options;
  const ports = [];
  for (let port = MCP_PORT_RANGE[0]; port <= MCP_PORT_RANGE[1]; port += 1) {
    ports.push(port);
  }
  if (preferredPort && ports.includes(preferredPort)) {
    ports.splice(ports.indexOf(preferredPort), 1);
    ports.unshift(preferredPort);
  }

  const open = [];
  for (let index = 0; index < ports.length; index += concurrency) {
    const batch = ports.slice(index, index + concurrency);
    const results = await Promise.all(
      batch.map(async (port) => [port, await probeTcp(port, "127.0.0.1", timeoutMs)]),
    );
    for (const [port, ok] of results) {
      if (ok) open.push(port);
    }
  }

  for (const port of open) {
    try {
      const url = `http://127.0.0.1:${port}/mcp`;
      const probe = await probeMcp(url, { timeoutMs: 2_000 });
      return { ...probe, port, source: "scan" };
    } catch {
      // keep trying
    }
  }
  return null;
}

export async function discoverMcp(discovery, options = {}) {
  const { allowScan = true } = options;
  const mcp = discovery?.data?.mcp ?? null;
  if (mcp?.enabled === false) {
    return { reachable: false, enabled: false, reason: "disabled" };
  }

  const candidates = [];
  if (mcp?.endpoint) {
    candidates.push({ url: mcp.endpoint, port: mcp.port ?? null, source: "discovery" });
  }
  const preferredPort = mcp?.preferredPort ?? MCP_PORT_RANGE[0];
  const preferredUrl = `http://127.0.0.1:${preferredPort}/mcp`;
  if (!candidates.some((entry) => entry.url === preferredUrl)) {
    candidates.push({ url: preferredUrl, port: preferredPort, source: "preferredPort" });
  }

  for (const candidate of candidates) {
    try {
      const probe = await probeMcp(candidate.url, { timeoutMs: 2_000 });
      return {
        ...probe,
        port: candidate.port,
        source: candidate.source,
        reachable: true,
        enabled: true,
      };
    } catch {
      // try the next candidate
    }
  }

  if (allowScan && mcp?.enabled !== false) {
    const found = await scanForMcp(preferredPort);
    if (found) {
      return {
        ...found,
        reachable: true,
        enabled: true,
      };
    }
  }

  return {
    reachable: false,
    enabled: mcp?.enabled ?? "unknown",
    reason: mcp?.enabled === false ? "disabled" : "unreachable",
  };
}

function instructionsFor(nextAction, context) {
  const { platform, installHint } = context;
  switch (nextAction) {
    case "unsupported":
      return [platform.reason];
    case "install":
      if (installHint === "debian") {
        return [
          "运行 `szu-life install` 获取一条 deb 安装 bash 命令，把它原样交给用户执行。",
          "不要自己执行 sudo；用户执行完后用 `szu-life status` 验证。",
        ];
      }
      if (installHint === "arch") {
        return [
          "运行 `szu-life install` 获取 AUR 安装 bash 命令，把它原样交给用户执行。",
          "不要自己执行 yay/paru/makepkg；用户执行完后用 `szu-life status` 验证。",
        ];
      }
      return [
        "先向用户介绍泥深并征得同意，然后运行 `szu-life install --yes`。",
        installHint === "msi"
          ? "Windows 会弹出 UAC，请提前说明。"
          : "其它 Linux 发行版会把 AppImage 装到用户 XDG 目录，不需要 sudo。",
      ];
    case "launch":
      return ["泥深已安装但未运行；调用 `szu_life_ensure_ready` 或 `szu-life launch` 启动。"];
    case "onboarding":
      return [
        "打开泥深并让用户完成初始化向导（初始化未完成前所有页面都会被向导挡住）。",
        "用户完成后再次调用 `szu_life_status`。",
      ];
    case "auth":
      return [
        "统一认证未完成或已失效；打开泥深让用户在官方认证窗口登录。",
        "不要询问或处理校园卡密码。",
      ];
    case "enable-mcp":
      return [
        `打开 ${DEEP_LINKS.mcpSettings}，让用户打开「开启 MCP 服务端口」开关。`,
        "用户操作后再次调用 `szu_life_ensure_ready`。",
      ];
    case "mcp-unreachable":
      return ["MCP 已开启但没有探测到端口；让用户检查设置页的端口提示，必要时关闭再打开开关。"];
    case "uooc-login":
      return [
        `打开 ${DEEP_LINKS.uooc}，让用户点「登录 UOOC」并在官方窗口完成登录。`,
        "登录完成后再次调用 `szu_life_status`。",
      ];
    case "ready":
      return ["泥深 MCP 已就绪，可以调用应用工具完成用户请求。"];
    default:
      return [];
  }
}

export async function collectStatus(options = {}) {
  const { need = "general", allowScan = true } = options;
  const platform = detectPlatform();
  const installMeta = await readInstallMeta();
  const binary = await findAppBinary(installMeta?.value);
  const distro = process.platform === "linux" ? await detectLinuxDistro() : null;

  let install = null;
  if (installMeta?.value?.version) {
    install = {
      version: installMeta.value.version,
      channel: installMeta.value.channel ?? null,
      format: installMeta.value.format ?? null,
      source: installMeta.value.source ?? null,
    };
  }

  const packageInfo = await linuxPackageVersion(binary, distro?.family);
  if (packageInfo) {
    install = {
      version: packageInfo.version,
      channel: install?.channel ?? null,
      format: install?.format ?? (packageInfo.manager === "deb" ? "deb" : "aur"),
      source: install?.source ?? packageInfo.manager,
      package: packageInfo.package,
    };
  }

  const discovery = await readIntegrationFile();
  const discoveryData = discovery?.value ?? null;
  const validDiscovery =
    discoveryData &&
    typeof discoveryData === "object" &&
    Number.isInteger(discoveryData.schemaVersion) &&
    discoveryData.schemaVersion >= 1
      ? discoveryData
      : null;

  const installed = Boolean(binary);
  const processInfo = await findAppProcess();
  const running =
    Boolean(processInfo) || Boolean(validDiscovery?.pid && isPidAlive(validDiscovery.pid));
  const pid = processInfo?.pid ?? validDiscovery?.pid ?? null;

  if (!install && validDiscovery?.app?.version) {
    install = {
      version: validDiscovery.app.version,
      channel: null,
      format: null,
      source: "integration.json",
    };
  }
  if (!install && installed) {
    install = {
      version: null,
      channel: null,
      format:
        platform.os === "win32"
          ? "msi"
          : distro?.family === "debian"
            ? "deb"
            : distro?.family === "arch"
              ? "aur"
              : "appimage",
      source: "filesystem",
    };
  }

  const mcpInfo = validDiscovery?.mcp ?? null;
  let mcp = {
    enabled: mcpInfo?.enabled ?? "unknown",
    running: mcpInfo?.running ?? false,
    port: mcpInfo?.port ?? null,
    preferredPort: mcpInfo?.preferredPort ?? MCP_PORT_RANGE[0],
    url: mcpInfo?.endpoint ?? null,
    reachable: false,
    serverName: null,
    serverVersion: null,
    source: null,
  };

  let client = null;
  if (running) {
    const found = await discoverMcp({ data: validDiscovery ?? { mcp: mcpInfo } }, { allowScan });
    if (found.reachable) {
      mcp = {
        ...mcp,
        enabled: true,
        running: true,
        port: found.port ?? mcp.port,
        url: found.url,
        reachable: true,
        serverName: found.info?.name ?? APP_NAME,
        serverVersion: found.info?.version ?? null,
        source: found.source,
      };
      client = found.client;
    } else {
      mcp = {
        ...mcp,
        running: mcpInfo?.running ?? false,
        reachable: false,
        source: null,
      };
    }
  }

  let onboarding = validDiscovery?.onboarding ?? null;
  let auth = validDiscovery?.auth ?? null;
  let uooc = validDiscovery?.uooc ?? null;

  // Older app versions do not write integration.json. If MCP is reachable,
  // query the existing onboarding/auth/UOOC surfaces instead of guessing.
  if ((!onboarding || !auth || !uooc) && client) {
    const remote = await remoteStatusFromMcp(client);
    onboarding = onboarding ?? remote.onboarding;
    auth = auth ?? remote.auth;
    uooc = uooc ?? remote.uooc;
  }

  const installHint =
    platform.os === "win32"
      ? "msi"
      : distro?.family === "debian"
        ? "debian"
        : distro?.family === "arch"
          ? "arch"
          : "appimage";

  let nextAction = "ready";
  if (!platform.supported) nextAction = "unsupported";
  else if (!installed) nextAction = "install";
  else if (!running) nextAction = "launch";
  else if (onboarding?.available && onboarding.needsSetup === true) {
    nextAction = "onboarding";
  } else if (auth && auth.loggedIn === false) {
    nextAction = "auth";
  } else if (mcp.enabled === false) {
    nextAction = "enable-mcp";
  } else if (!mcp.reachable) {
    nextAction = "mcp-unreachable";
  } else if (need === "uooc" && uooc && uooc.loggedIn === false) {
    nextAction = "uooc-login";
  }

  const status = {
    ok: true,
    supported: platform.supported,
    platform: {
      os: platform.os,
      arch: platform.arch,
      desktop: platform.desktop,
      reason: platform.reason || undefined,
      archNote: platform.archNote,
    },
    linux: distro
      ? {
          family: distro.family,
          id: distro.id,
          idLike: distro.idLike,
          prettyName: distro.prettyName,
        }
      : undefined,
    installed,
    install,
    running,
    pid,
    app: validDiscovery
      ? {
          version: validDiscovery.app?.version ?? null,
          schemaVersion: validDiscovery.schemaVersion,
          updatedAtMs: validDiscovery.updatedAtMs ?? null,
          startedAtMs: validDiscovery.startedAtMs ?? null,
        }
      : null,
    mcp,
    onboarding,
    auth,
    uooc,
    nextAction,
    installHint,
    instructions: instructionsFor(nextAction, { platform, installHint }),
  };

  return { status, client, binary, discovery, distro };
}

export async function mcpEndpoint() {
  const { status, client } = await collectStatus({ allowScan: true });
  return {
    reachable: status.mcp.reachable,
    url: status.mcp.url,
    port: status.mcp.port,
    serverName: status.mcp.serverName,
    serverVersion: status.mcp.serverVersion,
    source: status.mcp.source,
    client,
  };
}
