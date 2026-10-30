// 手动触发：把 CNB 侧某个分支推送到 GitHub 的同源分支（cnb/<branch>）并创建 PR。
// GitHub 的受保护分支（main）不接受直接推送，因此这里只推 cnb/* 分支，再用 API 建 PR。
//
// 需要的环境变量：
//   GITHUB_REPO         GitHub 仓库，默认 rinnein/your-szu-life
//   GITHUB_SYNC_USERNAME GitHub 用户名（PAT 的持有者）
//   GITHUB_SYNC_TOKEN   GitHub PAT（需要 contents:rw + pull_requests:rw，经典 PAT 为 repo 范围）
// 可选（来自 .cnb/web_trigger.yml 的 inputs，或手工 export）：
//   source_branch       CNB 侧源分支，默认取当前分支（CNB_BRANCH）
//   base_branch         GitHub 侧目标分支，默认 main
//   pr_title            自定义 PR 标题
//   DRY_RUN=true        只打印计划，不推送也不创建 PR
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

async function main() {
  if (!SOURCE_BRANCH) throw new Error("无法确定源分支：请在手动触发时填写 source_branch");

  const headBranch = `cnb/${SOURCE_BRANCH.replace(/[^\w.-]+/g, "-")}`;
  const remoteUrl = `https://github.com/${GH_REPO}.git`;
  const [owner] = GH_REPO.split("/");

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
    log("没有需要同步的提交，跳过推送与 PR 创建");
    return;
  }

  const commitLines = git(["log", "--format=%h %s", "--max-count=50", `refs/remotes/github/${BASE_BRANCH}..${sha}`])
    .split("\n")
    .filter(Boolean);

  if (DRY_RUN) {
    log(`dry-run：将推送 ${sha} 到 ${headBranch} 并创建 PR`);
    log(commitLines.join("\n"));
    return;
  }

  if (!GH_TOKEN) throw new Error("缺少环境变量 GITHUB_SYNC_TOKEN（GitHub PAT）");

  withCredentialHelper((helper) => {
    git(["-c", helper, "push", "--force", "github", `${sha}:refs/heads/${headBranch}`]);
  });
  log(`已推送 ${sha} → ${headBranch}`);

  const title = PR_TITLE || `chore: sync ${SOURCE_BRANCH} from CNB`;
  const body = buildBody(commitLines, ahead, headBranch);

  const existing = await ghJson(
    `/repos/${GH_REPO}/pulls?state=open&head=${encodeURIComponent(`${owner}:${headBranch}`)}&base=${BASE_BRANCH}`,
  );
  if (existing.ok && Array.isArray(existing.data) && existing.data.length > 0) {
    const number = existing.data[0].number;
    const updated = await ghJson(`/repos/${GH_REPO}/pulls/${number}`, {
      method: "PATCH",
      body: JSON.stringify({ title, body }),
    });
    if (!updated.ok) throw new Error(`更新 PR #${number} 失败: ${JSON.stringify(updated.data)}`);
    log(`已更新已有 PR #${number}: ${updated.data.html_url}`);
    console.log(`##[set-output pr_url=${updated.data.html_url}]`);
    return;
  }

  const created = await ghJson(`/repos/${GH_REPO}/pulls`, {
    method: "POST",
    body: JSON.stringify({ title, body, head: headBranch, base: BASE_BRANCH, maintainer_can_modify: true }),
  });
  if (!created.ok) {
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
