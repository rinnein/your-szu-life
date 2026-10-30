// 手动触发：把 CNB 侧某个分支推送到 GitHub 的同源分支（cnb/<branch>），
// 需要时创建 PR，PR 已存在则更新它（分支内容 + 标题/描述）。
// GitHub 的受保护分支（main）不接受直接推送，因此这里只推 cnb/* 分支，再用 API 建/改 PR。
//
// 需要的环境变量：
//   GITHUB_REPO         GitHub 仓库，默认 rinnein/your-szu-life
//   GITHUB_SYNC_USERNAME GitHub 用户名（PAT 的持有者）
//   GITHUB_SYNC_TOKEN   GitHub PAT（需要 contents:rw + pull_requests:rw，经典 PAT 为 repo 范围）
// 可选（来自 .cnb/web_trigger.yml 的 inputs，或手工 export）：
//   source_branch       CNB 侧源分支，默认取当前分支（CNB_BRANCH）
//   base_branch         GitHub 侧目标分支，默认 main
//   pr_title            自定义 PR 标题
//   DRY_RUN=true        只打印计划，不推送也不创建/更新 PR
import { execFileSync } from "node:child_process";
import { rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const GH_API = "https://api.github.com";
const GH_REPO = process.env.GITHUB_REPO || "rinnein/your-szu-life";
const GH_TOKEN = process.env.GITHUB_SYNC_TOKEN || "";
const SOURCE_BRANCH = (process.env.source_branch || process.env.CNB_BRANCH || "").trim();
const BASE_BRANCH = (process.env.base_branch || "main").trim();
const PR_TITLE = (process.env.pr_title || "").trim();
const DRY_RUN = process.argv.includes("--dry-run") || /^(1|true|yes)$/i.test(process.env.DRY_RUN || "");

// 自动生成的标题前缀：用来判断现有 PR 的标题是否被人工改过。
const AUTO_TITLE_PREFIX = "chore: sync ";

const log = (...args) => console.log("[push-pr]", ...args);

function git(args, options = {}) {
  return execFileSync("git", args, {
    encoding: "utf8",
    env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
    ...options,
  }).trim();
}

function ghHeaders() {
  return {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    Authorization: `Bearer ${GH_TOKEN}`,
  };
}

async function ghJson(path, init = {}) {
  const headers = ghHeaders();
  if (init.body) headers["Content-Type"] = "application/json";
  const response = await fetch(`${GH_API}${path}`, { ...init, headers });
  const text = await response.text();
  let data = text;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    // 非 JSON（如代理返回的 HTML 错误页）时保留原文，方便排查。
  }
  return { ok: response.ok, status: response.status, data };
}

// git 走 credential helper，避免把 token 写进 URL（URL 会出现在日志里）。
function withCredentialHelper(fn) {
  const helper = join(tmpdir(), `gh-cred-${process.pid}.sh`);
  writeFileSync(
    helper,
    '#!/bin/sh\n[ "$1" = "get" ] || exit 0\necho "username=x-access-token"\necho "password=$GITHUB_SYNC_TOKEN"\n',
    { mode: 0o700 },
  );
  try {
    return fn(`credential.helper=${helper}`);
  } finally {
    rmSync(helper, { force: true });
  }
}

function autoTitle() {
  return `${AUTO_TITLE_PREFIX}${SOURCE_BRANCH} from CNB`;
}

function buildBody(commitLines, ahead, headBranch) {
  const buildUrl = process.env.CNB_BUILD_WEB_URL || "";
  const lines = [
    `由 CNB 流水线自动创建的同步 PR。`,
    "",
    `- 来源：\`${SOURCE_BRANCH}\`（CNB: ${process.env.CNB_REPO_SLUG || ""}）`,
    `- 目标：\`${BASE_BRANCH}\`（GitHub: ${GH_REPO}）`,
    `- 对比分支：\`${headBranch}\``,
    `- 提交数：${ahead}`,
  ];
  if (buildUrl) lines.push(`- 流水线：${buildUrl}`);
  lines.push("", "提交列表：", "", "```", ...commitLines, "```");
  if (commitLines.length < ahead) lines.push("", `> 提交较多，仅列出前 ${commitLines.length} 条。`);
  return lines.join("\n");
}

// 同一个 head/base 理论上只有一个 PR，但历史上可能关掉又重开过，所以查 state=all。
async function findPull(headBranch) {
  const [owner] = GH_REPO.split("/");
  const head = encodeURIComponent(`${owner}:${headBranch}`);
  const response = await ghJson(
    `/repos/${GH_REPO}/pulls?state=all&head=${head}&base=${BASE_BRANCH}&sort=created&direction=desc`,
  );
  if (!response.ok) throw new Error(`查询 PR 失败（head=${headBranch}）: ${JSON.stringify(response.data)}`);
  const pulls = Array.isArray(response.data) ? response.data : [];
  if (pulls.length === 0) return null;
  const open = pulls.filter((pull) => pull.state === "open");
  if (open.length > 1) log(`注意：${headBranch} 上有 ${open.length} 个未关闭 PR，只更新最新的 #${open[0].number}`);
  return open[0] ?? pulls[0];
}

async function patchPull(number, { title, body }) {
  const response = await ghJson(`/repos/${GH_REPO}/pulls/${number}`, {
    method: "PATCH",
    body: JSON.stringify({ title, body }),
  });
  if (!response.ok) throw new Error(`更新 PR #${number} 失败: ${JSON.stringify(response.data)}`);
  return response.data;
}

async function main() {
  if (!SOURCE_BRANCH) throw new Error("无法确定源分支：请在手动触发时填写 source_branch");

  const headBranch = `cnb/${SOURCE_BRANCH.replace(/[^\w.-]+/g, "-")}`;
  const remoteUrl = `https://github.com/${GH_REPO}.git`;

  log(`源分支 ${SOURCE_BRANCH} → GitHub ${headBranch} → PR 到 ${BASE_BRANCH}`);

  git(["fetch", "--no-tags", "origin", `+refs/heads/${SOURCE_BRANCH}:refs/remotes/origin/${SOURCE_BRANCH}`]);
  const sha = git(["rev-parse", `origin/${SOURCE_BRANCH}`]);

  if (git(["remote"]).split("\n").includes("github")) git(["remote", "set-url", "github", remoteUrl]);
  else git(["remote", "add", "github", remoteUrl]);
  // 有 token 就走 credential helper；没有时退化为匿名拉取（公开仓库可用）。
  const fetchBase = (helperPrefix = []) =>
    git([
      ...helperPrefix,
      "fetch",
      "--no-tags",
      "github",
      `+refs/heads/${BASE_BRANCH}:refs/remotes/github/${BASE_BRANCH}`,
    ]);
  if (GH_TOKEN) withCredentialHelper((helper) => fetchBase(["-c", helper]));
  else fetchBase();

  const ahead = Number(git(["rev-list", "--count", `refs/remotes/github/${BASE_BRANCH}..${sha}`]));
  log(`相对 github/${BASE_BRANCH} 有 ${ahead} 个提交`);
  if (ahead === 0) {
    log("没有需要同步的提交，跳过推送与 PR 更新");
    return;
  }

  const commitLines = git(["log", "--format=%h %s", "--max-count=50", `refs/remotes/github/${BASE_BRANCH}..${sha}`])
    .split("\n")
    .filter(Boolean);

  const existing = GH_TOKEN ? await findPull(headBranch) : null;
  const reuseExisting = existing?.state === "open";
  if (reuseExisting) log(`检测到已存在 PR #${existing.number}（${existing.html_url}），本次将更新它`);

  if (DRY_RUN) {
    log(`dry-run：将 force push ${sha} → ${headBranch}，然后${reuseExisting ? "更新" : "创建"} PR`);
    log(commitLines.join("\n"));
    return;
  }

  if (!GH_TOKEN) throw new Error("缺少环境变量 GITHUB_SYNC_TOKEN（GitHub PAT）");

  withCredentialHelper((helper) => {
    git(["-c", helper, "push", "--force", "github", `${sha}:refs/heads/${headBranch}`]);
  });
  log(`已推送 ${sha} → ${headBranch}`);

  // 人工改过的标题不要被自动标题覆盖；描述是我们自己的（含提交列表与构建链接），每次重建。
  const keepEditedTitle = reuseExisting && Boolean(existing.title) && !existing.title.startsWith(AUTO_TITLE_PREFIX);
  const title = PR_TITLE || (keepEditedTitle ? existing.title : autoTitle());
  const body = buildBody(commitLines, ahead, headBranch);

  if (reuseExisting) {
    // 标题/描述每次重建：正文里带本次提交列表和本次构建链接，重复运行是有意义的刷新。
    const previousSha = (existing.head?.sha || "").slice(0, 8);
    log(`更新 PR #${existing.number} 的标题/描述${previousSha ? `（分支 ${previousSha} → ${sha.slice(0, 8)}）` : ""}`);
    const updated = await patchPull(existing.number, { title, body });
    log(`已更新 PR #${existing.number}: ${updated.html_url}`);
    console.log(`##[set-output pr_url=${updated.html_url}]`);
    return;
  }

  if (existing) {
    log(`原 PR #${existing.number} 已${existing.merged_at ? "合并" : "关闭"}，改为新建 PR`);
  }

  const created = await ghJson(`/repos/${GH_REPO}/pulls`, {
    method: "POST",
    body: JSON.stringify({ title, body, head: headBranch, base: BASE_BRANCH, maintainer_can_modify: true }),
  });
  if (!created.ok) {
    // 422 "A pull request already exists"：并发的另一次运行刚好建好了，回查后改成更新。
    if (created.status === 422) {
      const raced = await findPull(headBranch);
      if (raced?.state === "open") {
        const updated = await patchPull(raced.number, { title, body });
        log(`PR #${raced.number} 已存在（并发创建），改为更新: ${updated.html_url}`);
        console.log(`##[set-output pr_url=${updated.html_url}]`);
        return;
      }
    }
    throw new Error(
      `创建 PR 失败: ${JSON.stringify(created.data)}` +
        `（请确认 PAT 有 pull_requests:rw，且 ${headBranch} 与 ${BASE_BRANCH} 都存在）`,
    );
  }
  log(`已创建 PR #${created.data.number}: ${created.data.html_url}`);
  console.log(`##[set-output pr_url=${created.data.html_url}]`);
}

main().catch((error) => {
  console.error("[push-pr] 失败：", error.message || error);
  process.exit(1);
});
