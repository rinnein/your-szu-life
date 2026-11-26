// Release metadata, channel/asset selection, and download candidate ordering.
//
// CNB is the primary source in mainland China and also exposes SHA-256 hashes.
// GitHub is the fallback, including the gh-proxy accelerators used by the app.

import {
  CNB_RELEASES_API,
  GITHUB_ACCELERATORS,
  GITHUB_RELEASES_API,
  GITHUB_RELEASES_PAGE,
  REPO,
  CNB_REPO,
} from "./constants.mjs";
import { fetchJson, fetchText } from "./util.mjs";

export function normalizeReleases(raw) {
  const list = Array.isArray(raw)
    ? raw
    : Array.isArray(raw?.releases)
      ? raw.releases
      : Array.isArray(raw?.data)
        ? raw.data
        : [];
  return list
    .map((entry) => normalizeRelease(entry))
    .filter((entry) => entry && entry.tag && entry.tag !== "announcement");
}

export function normalizeRelease(raw) {
  if (!raw || typeof raw !== "object") return null;
  const tag = String(raw.tag_name ?? raw.tagName ?? raw.tag ?? raw.name ?? "").trim();
  if (!tag) return null;
  const assets = Array.isArray(raw.assets) ? raw.assets : [];
  return {
    tag,
    title: raw.title ?? raw.name ?? tag,
    body: raw.body ?? raw.description ?? "",
    draft: Boolean(raw.draft),
    prerelease: Boolean(raw.prerelease),
    isLatest: Boolean(raw.is_latest ?? raw.isLatest),
    assets: assets.map((asset) => normalizeAsset(asset)).filter((asset) => asset && asset.name),
  };
}

function normalizeAsset(raw) {
  if (!raw || typeof raw !== "object") return null;
  const name = String(raw.name ?? "").trim();
  if (!name) return null;
  const url = String(
    raw.browser_download_url ?? raw.brower_download_url ?? raw.browserDownloadUrl ?? raw.url ?? "",
  ).trim();
  const hashAlgo = String(raw.hash_algo ?? "").toLowerCase();
  const sha256 =
    hashAlgo === "sha256" && typeof raw.hash_value === "string"
      ? raw.hash_value.toLowerCase()
      : typeof raw.sha256 === "string"
        ? raw.sha256.toLowerCase()
        : undefined;
  return {
    name,
    url,
    size: Number.isFinite(raw.size) ? Number(raw.size) : undefined,
    sha256,
  };
}

export function versionFromTag(tag) {
  const match = String(tag ?? "").match(/(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)/);
  return match ? match[1] : null;
}

export function extractVersionFromAsset(name) {
  const marker = String(name ?? "").indexOf("-release.");
  if (marker < 0) return null;
  const prefix = String(name).slice(0, marker);
  const underscore = prefix.lastIndexOf("_");
  if (underscore < 0) return null;
  const candidate = prefix.slice(underscore + 1);
  return /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(candidate) ? candidate : null;
}

export function compareVersions(left, right) {
  const parse = (value) =>
    String(value ?? "")
      .split(/[.+-]/)
      .map((part) => Number.parseInt(part, 10))
      .filter((part) => Number.isFinite(part));
  const a = parse(left);
  const b = parse(right);
  const length = Math.max(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    const diff = (a[index] ?? 0) - (b[index] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

export function isCanaryRelease(release) {
  return String(release?.tag ?? "")
    .toLowerCase()
    .includes("canary");
}

export function isStableRelease(release) {
  return Boolean(
    release &&
    !release.draft &&
    !release.prerelease &&
    !isCanaryRelease(release) &&
    versionFromTag(release.tag),
  );
}

function extensionMatches(name, format) {
  const lower = name.toLowerCase();
  if (format === "msi") return lower.endsWith(".msi");
  if (format === "deb") return lower.endsWith(".deb");
  if (format === "appimage") return lower.endsWith(".appimage");
  return lower.endsWith(".appimage") || lower.endsWith(".deb");
}

function assetMatchesPlatform(name, platform, arch) {
  const lower = name.toLowerCase();
  if (!lower.startsWith("your-szu-life_")) return false;
  const platformToken = platform === "windows" ? "_windows_" : "_linux_";
  if (!lower.includes(platformToken)) return false;
  return lower.includes(`_${arch.toLowerCase()}_`);
}

export function findAsset(release, { platform, arch, format = "auto" }) {
  const assets = release?.assets ?? [];
  const formats =
    format === "auto" ? (platform === "linux" ? ["appimage", "deb"] : ["msi"]) : [format];
  for (const candidateFormat of formats) {
    for (const asset of assets) {
      if (!assetMatchesPlatform(asset.name, platform, arch)) continue;
      if (!extensionMatches(asset.name, candidateFormat)) continue;
      return asset;
    }
  }
  return null;
}

/**
 * Select the newest release (stable first, canary fallback) that actually has
 * an asset for the requested platform/format.
 */
export function selectRelease(releases, options) {
  const { platform, arch, channel = "stable", format = "auto" } = options;
  const candidates = (releases ?? [])
    .filter((release) => release && release.assets?.length)
    .filter((release) => release.tag !== "announcement");

  const byVersionDesc = (left, right) => {
    const leftVersion =
      versionFromTag(left.tag) ?? extractVersionFromAsset(left.assets[0]?.name) ?? "0.0.0";
    const rightVersion =
      versionFromTag(right.tag) ?? extractVersionFromAsset(right.assets[0]?.name) ?? "0.0.0";
    return compareVersions(rightVersion, leftVersion);
  };

  const stable = candidates.filter(isStableRelease).sort(byVersionDesc);
  const canary = candidates
    .filter((release) => isCanaryRelease(release))
    .sort((left, right) => {
      const leftVersion = extractVersionFromAsset(left.assets?.[0]?.name) ?? "0.0.0";
      const rightVersion = extractVersionFromAsset(right.assets?.[0]?.name) ?? "0.0.0";
      return compareVersions(rightVersion, leftVersion);
    });

  const ordered = channel === "canary" ? [...canary, ...stable] : [...stable, ...canary];

  for (const release of ordered) {
    const asset = findAsset(release, { platform, arch, format });
    if (!asset) continue;
    const version = versionFromTag(release.tag) ?? extractVersionFromAsset(asset.name);
    if (!version) continue;
    return {
      release,
      asset,
      version,
      channel: isCanaryRelease(release) ? "canary" : "stable",
      fallback: channel === "stable" && isCanaryRelease(release),
      format: asset.name.toLowerCase().endsWith(".msi")
        ? "msi"
        : asset.name.toLowerCase().endsWith(".deb")
          ? "deb"
          : "appimage",
    };
  }
  return null;
}

export function buildDownloadCandidates(selected) {
  const tag = selected.release.tag;
  const name = selected.asset.name;
  const candidates = [];

  const add = (url) => {
    if (url && !candidates.includes(url)) candidates.push(url);
  };

  // CNB first (fastest in mainland China). Prefer the URL returned by the API.
  if (selected.asset.url && selected.asset.url.includes("cnb.cool")) {
    add(selected.asset.url);
  }
  add(
    `https://cnb.cool/${CNB_REPO}/-/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`,
  );

  const githubUrl = `https://github.com/${REPO}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
  for (const accelerator of GITHUB_ACCELERATORS) {
    add(`${accelerator}${githubUrl}`);
  }
  add(githubUrl);

  // Some APIs return an alternate direct URL (e.g. GitHub browser_download_url).
  if (selected.asset.url && /^https:\/\//i.test(selected.asset.url)) {
    add(selected.asset.url);
  }

  return candidates;
}

export async function fetchReleases(options = {}) {
  const { timeoutMs = 20_000 } = options;
  const errors = [];

  try {
    const raw = await fetchJson(CNB_RELEASES_API, { timeoutMs });
    const releases = normalizeReleases(raw);
    if (releases.length) return { releases, source: "cnb" };
    errors.push("CNB 没有返回任何 Release");
  } catch (error) {
    errors.push(`CNB: ${String(error)}`);
  }

  try {
    const raw = await fetchJson(GITHUB_RELEASES_API, {
      timeoutMs,
      headers: { "User-Agent": "your-szu-life-workbuddy" },
    });
    const releases = normalizeReleases(raw);
    if (releases.length) return { releases, source: "github" };
    errors.push("GitHub 没有返回任何 Release");
  } catch (error) {
    errors.push(`GitHub API: ${String(error)}`);
  }

  try {
    const html = await fetchText(GITHUB_RELEASES_PAGE, { timeoutMs });
    const releases = parseGithubReleaseHtml(html);
    if (releases.length) return { releases, source: "github-html" };
    errors.push("GitHub Releases 页面没有解析到资产");
  } catch (error) {
    errors.push(`GitHub HTML: ${String(error)}`);
  }

  throw new Error(`无法获取泥深发布信息：${errors.join("；")}`);
}

export function parseGithubReleaseHtml(html) {
  const text = String(html ?? "");
  const byTag = new Map();

  for (const match of text.matchAll(/\/releases\/tag\/([^"?#<>]+)/g)) {
    const tag = decodeURIComponent(match[1]);
    if (!byTag.has(tag)) byTag.set(tag, new Set());
  }
  for (const match of text.matchAll(/\/releases\/download\/([^/"?#<>]+)\/([^"?#<>]+)/g)) {
    const tag = decodeURIComponent(match[1]);
    const name = decodeURIComponent(match[2]);
    if (!byTag.has(tag)) byTag.set(tag, new Set());
    byTag.get(tag).add(name);
  }

  return [...byTag.entries()]
    .filter(([, assets]) => assets.size > 0)
    .map(([tag, assets]) => ({
      tag,
      title: tag,
      body: "",
      draft: false,
      prerelease: tag.toLowerCase().includes("canary"),
      isLatest: false,
      assets: [...assets].map((name) => ({
        name,
        url: `https://github.com/${REPO}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`,
      })),
    }));
}

export async function resolveInstallPlan(options) {
  const { platform, arch, channel = "stable", format = "auto" } = options;
  const { releases, source } = await fetchReleases();
  const selected = selectRelease(releases, { platform, arch, channel, format });
  if (!selected) {
    throw new Error(
      `没有找到适用于 ${platform}/${arch} 的 ${format === "auto" ? "" : `${format} `}安装包（频道 ${channel}）。`,
    );
  }
  return {
    ...selected,
    source,
    candidates: buildDownloadCandidates(selected),
  };
}
