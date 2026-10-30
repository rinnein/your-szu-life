// 把 GitHub Release（标题、说明、附件）镜像到 CNB Release。
// 代码与 tag 由 tencentcom/git-sync 插件同步，本脚本只处理 Release 本身。
//
// 需要的环境变量：
//   CNB_TOKEN          CNB 流水线临时令牌（自动注入，需要 release:rw）
//   CNB_API_ENDPOINT   CNB OpenAPI 地址（自动注入）
//   CNB_REPO_SLUG      当前仓库路径（自动注入）
//   GITHUB_REPO        GitHub 仓库，默认 rinnein/your-szu-life
//   GITHUB_SYNC_TOKEN  GitHub PAT（只读公共仓库时可省略）
// 可选：
//   DRY_RUN=true       只打印同步计划，不写入
//   SYNC_DRAFTS=true   同时同步草稿 Release
//   ONLY_TAGS=a,b      只处理指定 tag
import { createReadStream, createWriteStream } from "node:fs";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const GH_API = "https://api.github.com";
const CNB_API = (process.env.CNB_API_ENDPOINT || "https://api.cnb.cool").replace(/\/+$/, "");
const CNB_REPO = process.env.CNB_REPO_SLUG || "";
const CNB_TOKEN = process.env.CNB_TOKEN || "";
const GH_REPO = process.env.GITHUB_REPO || "rinnein/your-szu-life";
const GH_TOKEN = process.env.GITHUB_SYNC_TOKEN || process.env.GITHUB_TOKEN || "";
const DRY_RUN = process.argv.includes("--dry-run") || /^(1|true|yes)$/i.test(process.env.DRY_RUN || "");
const SYNC_DRAFTS = /^(1|true|yes)$/i.test(process.env.SYNC_DRAFTS || "");
const ONLY_TAGS = (process.env.ONLY_TAGS || "")
  .split(",")
  .map((tag) => tag.trim())
  .filter(Boolean);

const log = (...args) => console.log("[release-sync]", ...args);
const warn = (...args) => console.log("[release-sync][warn]", ...args);

function ghHeaders(extra = {}) {
  const headers = { Accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28", ...extra };
  if (GH_TOKEN) headers.Authorization = `Bearer ${GH_TOKEN}`;
  return headers;
}

function cnbHeaders(extra = {}) {
  // 缺少 Accept 时 CNB OpenAPI 会返回 406。
  return { Accept: "application/vnd.cnb.api+json", Authorization: `Bearer ${CNB_TOKEN}`, ...extra };
}

async function readError(response) {
  const text = await response.text().catch(() => "");
  return `${response.status} ${response.statusText} ${text.slice(0, 500)}`.trim();
}

async function ghJson(path) {
  const response = await fetch(`${GH_API}${path}`, { headers: ghHeaders() });
  if (!response.ok) throw new Error(`GitHub ${path} 请求失败: ${await readError(response)}`);
  return response.json();
}

async function cnbJson(path, init = {}) {
  const response = await fetch(`${CNB_API}${path}`, {
    ...init,
    headers: cnbHeaders(
      init.body === undefined ? (init.headers || {}) : { "Content-Type": "application/json", ...(init.headers || {}) },
    ),
  });
  if (!response.ok) throw new Error(`CNB ${path} 请求失败: ${await readError(response)}`);
  if (response.status === 204) return null;
  const text = await response.text();
  return text ? JSON.parse(text) : null;
}

async function listGithubReleases() {
  const releases = [];
  for (let page = 1; ; page += 1) {
    const batch = await ghJson(`/repos/${GH_REPO}/releases?per_page=100&page=${page}`);
    releases.push(...batch);
    if (batch.length < 100) break;
  }
  return releases;
}

async function listCnbReleases() {
  const releases = [];
  for (let page = 1; ; page += 1) {
    const batch = await cnbJson(`/${CNB_REPO}/-/releases?page=${page}&page_size=100`);
    if (!Array.isArray(batch) || batch.length === 0) break;
    releases.push(...batch);
    if (batch.length < 100) break;
  }
  return releases;
}

function parseDigest(digest) {
  if (typeof digest !== "string") return null;
  const [algo, value] = digest.split(":", 2);
  if (!value) return null;
  return { algo: algo.toLowerCase(), value: value.toLowerCase() };
}

function sameRelease(github, cnb, latestTag) {
  const differences = [];
  if ((github.name || github.tag_name) !== (cnb.name || "")) differences.push("标题");
  if ((github.body || "") !== (cnb.body || "")) differences.push("说明");
  if (Boolean(github.prerelease) !== Boolean(cnb.prerelease)) differences.push("预发布");
  if (Boolean(github.draft) !== Boolean(cnb.draft)) differences.push("草稿");
  if (latestTag && (github.tag_name === latestTag) !== Boolean(cnb.is_latest)) differences.push("最新版本标记");
  return differences;
}

function assetDifferences(githubAsset, cnbAsset) {
  const reasons = [];
  if (cnbAsset.size !== githubAsset.size) reasons.push(`大小 ${cnbAsset.size} → ${githubAsset.size}`);
  const digest = parseDigest(githubAsset.digest);
  if (digest && digest.algo === "sha256") {
    if (!cnbAsset.hash_value) reasons.push("缺少 sha256");
    else if (cnbAsset.hash_value.toLowerCase() !== digest.value) reasons.push("sha256 不同");
  }
  return reasons;
}

async function downloadGithubAsset(asset, dir) {
  const file = join(dir, `${asset.id}-${asset.name.replace(/[^\w.-]+/g, "_")}`);
  const response = await fetch(asset.url, { headers: ghHeaders({ Accept: "application/octet-stream" }) });
  if (!response.ok) throw new Error(`下载 ${asset.name} 失败: ${await readError(response)}`);
  await pipeline(Readable.fromWeb(response.body), createWriteStream(file));
  const { size } = await stat(file);
  if (asset.size && size !== asset.size) {
    throw new Error(`下载 ${asset.name} 大小不符: 期望 ${asset.size}，实际 ${size}`);
  }
  return file;
}

async function uploadAsset(releaseId, asset, dir) {
  const upload = await cnbJson(`/${CNB_REPO}/-/releases/${releaseId}/asset-upload-url`, {
    method: "POST",
    body: JSON.stringify({ asset_name: asset.name, size: asset.size, overwrite: true }),
  });
  const file = await downloadGithubAsset(asset, dir);
  try {
    const put = await fetch(upload.upload_url, {
      method: "PUT",
      headers: {
        "Content-Type": asset.content_type || "application/octet-stream",
        "Content-Length": String(asset.size),
      },
      body: createReadStream(file),
      duplex: "half",
    });
    if (!put.ok) throw new Error(`上传 ${asset.name} 失败: ${await readError(put)}`);
    const verify = await fetch(upload.verify_url, {
      method: "POST",
      headers: cnbHeaders({ "Content-Type": "application/json" }),
      body: "{}",
    });
    if (!verify.ok) throw new Error(`确认 ${asset.name} 失败: ${await readError(verify)}`);
  } finally {
    await rm(file, { force: true });
  }
}

async function syncRelease(github, cnb, latestTag, dir, summary) {
  const targetReleaseId = cnb ? cnb.id : null;
  const differences = cnb ? sameRelease(github, cnb, latestTag) : ["新建"];

  let releaseId = targetReleaseId;
  if (!cnb) {
    log(`+ 新建 Release ${github.tag_name}（${github.name || github.tag_name}）`);
    if (!DRY_RUN) {
      const created = await cnbJson(`/${CNB_REPO}/-/releases`, {
        method: "POST",
        body: JSON.stringify({
          tag_name: github.tag_name,
          target_commitish: github.target_commitish || process.env.CNB_DEFAULT_BRANCH || "main",
          name: github.name || github.tag_name,
          body: github.body || "",
          draft: Boolean(github.draft),
          prerelease: Boolean(github.prerelease),
          make_latest: github.tag_name === latestTag ? "true" : "false",
        }),
      });
      releaseId = created.id;
    }
    summary.created += 1;
  } else if (differences.length > 0) {
    log(`~ 更新 Release ${github.tag_name}：${differences.join("、")}`);
    if (!DRY_RUN) {
      await cnbJson(`/${CNB_REPO}/-/releases/${releaseId}`, {
        method: "PATCH",
        body: JSON.stringify({
          name: github.name || github.tag_name,
          body: github.body || "",
          draft: Boolean(github.draft),
          prerelease: Boolean(github.prerelease),
          make_latest: github.tag_name === latestTag ? "true" : "false",
        }),
      });
    }
    summary.updated += 1;
  } else {
    log(`= Release ${github.tag_name} 元数据一致`);
  }

  // 附件：以 GitHub 为基准做镜像，多的删、缺的补、内容变了的替换。
  const githubAssets = github.assets || [];
  const cnbAssets = new Map((cnb?.assets || []).map((asset) => [asset.name, asset]));
  const githubNames = new Set(githubAssets.map((asset) => asset.name));

  const toDelete = [];
  for (const [name, asset] of cnbAssets) {
    if (!githubNames.has(name)) {
      toDelete.push({ asset, reason: "GitHub 已无此附件" });
      continue;
    }
    const githubAsset = githubAssets.find((item) => item.name === name);
    const reasons = assetDifferences(githubAsset, asset);
    if (reasons.length > 0) toDelete.push({ asset, reason: reasons.join("、") });
  }

  const toUpload = [];
  for (const asset of githubAssets) {
    if (asset.state && asset.state !== "uploaded") {
      warn(`跳过未完成上传的 GitHub 附件 ${asset.name}（state=${asset.state}）`);
      continue;
    }
    const cnbAsset = cnbAssets.get(asset.name);
    if (!cnbAsset || assetDifferences(asset, cnbAsset).length > 0) toUpload.push(asset);
  }

  if (toDelete.length === 0 && toUpload.length === 0) {
    if (cnb) log(`= Release ${github.tag_name} 附件一致（${githubAssets.length} 个）`);
    return;
  }

  for (const { asset, reason } of toDelete) {
    log(`- 删除附件 ${asset.name}（${reason}）`);
    if (DRY_RUN || !releaseId) continue;
    await cnbJson(`/${CNB_REPO}/-/releases/${releaseId}/assets/${asset.id}`, { method: "DELETE" });
    summary.deletedAssets += 1;
  }

  for (const asset of toUpload) {
    log(`+ 上传附件 ${asset.name}（${(asset.size / 1048576).toFixed(1)} MiB）`);
    if (DRY_RUN || !releaseId) continue;
    await uploadAsset(releaseId, asset, dir);
    summary.uploadedAssets += 1;
  }

  if (!DRY_RUN && releaseId && toUpload.length > 0) {
    const detail = await cnbJson(`/${CNB_REPO}/-/releases/${releaseId}`);
    const uploaded = new Map((detail.assets || []).map((asset) => [asset.name, asset]));
    for (const asset of toUpload) {
      const remote = uploaded.get(asset.name);
      if (!remote) throw new Error(`附件 ${asset.name} 上传后未出现在 Release 中`);
      if (remote.size !== asset.size) {
        throw new Error(`附件 ${asset.name} 大小不符: GitHub ${asset.size}，CNB ${remote.size}`);
      }
      const digest = parseDigest(asset.digest);
      if (digest && digest.algo === "sha256" && remote.hash_value) {
        if (remote.hash_value.toLowerCase() !== digest.value) {
          throw new Error(`附件 ${asset.name} sha256 不符: GitHub ${digest.value}，CNB ${remote.hash_value}`);
        }
      }
    }
    log(`✓ Release ${github.tag_name} 附件已同步`);
  }
}

async function main() {
  if (!CNB_REPO) throw new Error("缺少环境变量 CNB_REPO_SLUG");
  if (!CNB_TOKEN) throw new Error("缺少环境变量 CNB_TOKEN");
  if (DRY_RUN) log("dry-run：只打印计划，不写入 CNB");

  const githubReleases = (await listGithubReleases()).filter((release) => {
    if (!SYNC_DRAFTS && release.draft) return false;
    if (ONLY_TAGS.length > 0 && !ONLY_TAGS.includes(release.tag_name)) return false;
    return true;
  });
  const cnbReleases = await listCnbReleases();
  log(`GitHub Release ${githubReleases.length} 个，CNB Release ${cnbReleases.length} 个`);

  // GitHub 的「latest」= 最新的非草稿、非预发布版本。
  const latestTag = githubReleases.find((release) => !release.draft && !release.prerelease)?.tag_name;

  const cnbByTag = new Map(cnbReleases.map((release) => [release.tag_name, release]));
  const summary = { created: 0, updated: 0, uploadedAssets: 0, deletedAssets: 0 };

  const dir = await mkdtemp(join(tmpdir(), "cnb-release-sync-"));
  try {
    for (const github of githubReleases) {
      let cnb = cnbByTag.get(github.tag_name);
      // 列表接口不保证带附件明细，比较前取一次详情。
      if (cnb) cnb = await cnbJson(`/${CNB_REPO}/-/releases/${cnb.id}`);
      await syncRelease(github, cnb, latestTag, dir, summary);
    }
  } finally {
    await rm(dir, { recursive: true, force: true });
  }

  for (const release of cnbReleases) {
    if (!githubReleases.some((item) => item.tag_name === release.tag_name)) {
      warn(`CNB 侧存在 GitHub 没有的 Release：${release.tag_name}（不会自动删除，请人工确认）`);
    }
  }

  log(
    `完成：新建 ${summary.created}，更新 ${summary.updated}，` +
      `上传附件 ${summary.uploadedAssets}，删除附件 ${summary.deletedAssets}`,
  );
  if (DRY_RUN) log("（dry-run：上述为计划，未写入 CNB）");
}

main().catch((error) => {
  console.error("[release-sync] 失败：", error);
  process.exit(1);
});
