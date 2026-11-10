// 发布（或覆盖）一个 GitHub Release，供稳定版与 canary 两条通道共用。
//
// 稳定版：TAG=<推上来的版本 tag>、PRERELEASE=false、MAKE_LATEST=true、
//         TARGET_COMMITISH=<构建提交>；正文优先取 BODY_FILE（changelogen 按
//         tag 差异生成的 commit 概要），其次 GitHub generate-notes。
// canary ：TAG=canary、PRERELEASE=true、MAKE_LATEST=false、MOVE_TAG=true，
//         把滚动 tag 强推到本次构建的提交。
//
// 同名 release 已存在时先删光旧资产再上传本次资产，所以「同一个 tag 重发」
// 等价于覆盖；最后再删一遍残留资产，防止半途失败留下混血内容。
//
// 参数全部走环境变量，token 只放在 Authorization 头里：
//   GH_TOKEN（必需）、GH_REPO（默认 GITHUB_REPOSITORY）、
//   TAG、TITLE、VERSION、PRERELEASE、MAKE_LATEST、TARGET_COMMITISH、
//   MOVE_TAG、GENERATE_NOTES、BODY、BODY_FILE、ASSET_DIR（默认 packages）、
//   DRY_RUN。BODY_FILE 不存在或读取失败时按日志回退到后续链路。
//
// 自检：DRY_RUN=1 GH_REPO=owner/repo TAG=canary ASSET_DIR=<有文件的目录> \
//       node .ci/publish-github-release.mjs

import { readFile, readdir } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

const env = process.env;
const DRY_RUN = isTrue(env.DRY_RUN);
const REPO = env.GH_REPO || env.GITHUB_REPOSITORY || "";
const TAG = env.TAG || "";
const VERSION = env.VERSION || "";
const PRERELEASE = isTrue(env.PRERELEASE);
const TITLE =
  env.TITLE || (TAG === "canary" ? `canary v${VERSION}` : VERSION ? `v${VERSION}` : TAG);
const MAKE_LATEST = isTrue(env.MAKE_LATEST);
const MOVE_TAG = isTrue(env.MOVE_TAG);
const TARGET_COMMITISH = env.TARGET_COMMITISH || env.GITHUB_SHA || "";
const GENERATE_NOTES = env.GENERATE_NOTES !== "false";
const ASSET_DIR = resolve(env.ASSET_DIR || "packages");

const API = "https://api.github.com";
const UPLOADS = "https://uploads.github.com";

function isTrue(value) {
  return value === "1" || value === "true" || value === "TRUE";
}

function log(message) {
  console.log(`[publish] ${message}`);
}

class GhError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

function required(name, value) {
  if (!value) throw new Error(`缺少环境变量 ${name}`);
  return value;
}

function ghHeaders(extra = {}) {
  return {
    Authorization: `Bearer ${required("GH_TOKEN", env.GH_TOKEN)}`,
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    ...extra,
  };
}

async function gh(path, init = {}) {
  const url = path.startsWith("http") ? path : `${API}${path}`;
  const response = await fetch(url, { ...init, headers: ghHeaders(init.headers) });
  if (!response.ok) {
    const text = await response.text();
    throw new GhError(response.status, `GitHub ${path} 请求失败 (${response.status}): ${text}`);
  }
  if (response.status === 204) return null;
  const text = await response.text();
  return text ? JSON.parse(text) : null;
}

async function collectAssets(dir) {
  const files = [];
  async function walk(current) {
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const file = join(current, entry.name);
      if (entry.isDirectory()) await walk(file);
      else if (entry.isFile()) files.push(file);
    }
  }
  await walk(dir);
  files.sort();
  if (files.length === 0) throw new Error(`${dir} 下没有任何待发布资产`);
  const assets = files.map((file) => ({ file, name: basename(file) }));
  if (new Set(assets.map((asset) => asset.name)).size !== assets.length) {
    throw new Error("待发布资产存在重名文件");
  }
  return assets;
}

async function listAssets(releaseId) {
  const assets = [];
  for (let page = 1; ; page += 1) {
    const batch = await gh(`/repos/${REPO}/releases/${releaseId}/assets?per_page=100&page=${page}`);
    assets.push(...batch);
    if (batch.length < 100) break;
  }
  return assets;
}

async function getReleaseByTag() {
  try {
    return await gh(`/repos/${REPO}/releases/tags/${encodeURIComponent(TAG)}`);
  } catch (error) {
    if (error instanceof GhError && error.status === 404) return null;
    throw error;
  }
}

function fallbackBody() {
  // Markdown 列表：应用内更新提醒按 Markdown 渲染，纯文本的行会被合并成一段。
  const bullets = [
    VERSION ? `- 版本：${VERSION}` : "",
    TARGET_COMMITISH ? `- 提交：\`${TARGET_COMMITISH}\`` : "",
    env.GITHUB_RUN_ID && env.GITHUB_SERVER_URL
      ? `- 构建：[Actions 运行](${env.GITHUB_SERVER_URL}/${REPO}/actions/runs/${env.GITHUB_RUN_ID})`
      : "",
  ].filter(Boolean);
  return [
    PRERELEASE ? "自动构建预发布。" : "自动构建正式发布。",
    ...(bullets.length ? ["", ...bullets] : []),
  ].join("\n");
}

async function resolveBody() {
  if (env.BODY) return env.BODY;
  if (env.BODY_FILE) {
    try {
      const body = (await readFile(env.BODY_FILE, "utf8")).trim();
      if (body) return body;
      log(`BODY_FILE ${env.BODY_FILE} 是空文件，退回自动正文`);
    } catch (error) {
      log(`BODY_FILE ${env.BODY_FILE} 读取失败，退回自动正文：${error.message}`);
    }
  }
  if (GENERATE_NOTES && !PRERELEASE && TAG && TAG !== "canary") {
    try {
      const notes = await gh(`/repos/${REPO}/releases/generate-notes`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          tag_name: TAG,
          ...(TARGET_COMMITISH ? { target_commitish: TARGET_COMMITISH } : {}),
        }),
      });
      if (notes?.body) return notes.body;
    } catch (error) {
      log(`generate-notes 失败，退回自动正文：${error.message}`);
    }
  }
  return fallbackBody();
}

async function moveTag() {
  const sha = required("TARGET_COMMITISH", TARGET_COMMITISH);
  try {
    await gh(`/repos/${REPO}/git/refs/tags/${encodeURIComponent(TAG)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sha, force: true }),
    });
    log(`已把 tag ${TAG} 强推到 ${sha}`);
  } catch (error) {
    if (!(error instanceof GhError) || (error.status !== 404 && error.status !== 422)) {
      throw error;
    }
    await gh(`/repos/${REPO}/git/refs`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ref: `refs/tags/${TAG}`, sha }),
    });
    log(`已创建 tag ${TAG} 指向 ${sha}`);
  }
}

async function publish() {
  required("GH_REPO", REPO);
  required("TAG", TAG);
  const assets = await collectAssets(ASSET_DIR);
  log(`待发布 ${assets.length} 个资产到 ${REPO}@${TAG}`);

  if (DRY_RUN) {
    for (const asset of assets) log(`dry-run：上传 ${asset.name}`);
    if (MOVE_TAG) log(`dry-run：把 tag ${TAG} 强推到 ${TARGET_COMMITISH || "<commit>"}`);
    log(`dry-run：name=${TITLE} prerelease=${PRERELEASE} make_latest=${MAKE_LATEST}`);
    return;
  }

  let release = await getReleaseByTag();
  const previousAssets = release ? await listAssets(release.id) : [];
  if (previousAssets.length > 0) {
    log(`先清掉 ${previousAssets.length} 个旧资产`);
    for (const asset of previousAssets) {
      await gh(`/repos/${REPO}/releases/assets/${asset.id}`, { method: "DELETE" });
    }
  }

  if (MOVE_TAG) await moveTag();

  const body = await resolveBody();
  const metadata = {
    name: TITLE,
    body,
    draft: false,
    prerelease: PRERELEASE,
    make_latest: MAKE_LATEST ? "true" : "false",
  };
  release = release
    ? await gh(`/repos/${REPO}/releases/${release.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(metadata),
      })
    : await gh(`/repos/${REPO}/releases`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          tag_name: TAG,
          ...(TARGET_COMMITISH ? { target_commitish: TARGET_COMMITISH } : {}),
          ...metadata,
        }),
      });

  for (const asset of assets) {
    const data = await readFile(asset.file);
    await gh(
      `${UPLOADS}/repos/${REPO}/releases/${release.id}/assets?name=${encodeURIComponent(asset.name)}`,
      {
        method: "POST",
        headers: {
          "Content-Type": "application/octet-stream",
          "Content-Length": String(data.length),
        },
        body: data,
      },
    );
    log(`上传 ${asset.name}（${data.length} 字节）`);
  }

  const keep = new Set(assets.map((asset) => asset.name));
  for (const asset of await listAssets(release.id)) {
    if (!keep.has(asset.name)) {
      await gh(`/repos/${REPO}/releases/assets/${asset.id}`, { method: "DELETE" });
      log(`删除残留资产 ${asset.name}`);
    }
  }

  log(`完成：${release.html_url}`);
}

publish().catch((error) => {
  console.error("[publish] 失败：", error);
  process.exit(1);
});
