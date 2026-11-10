// PR 构建签名决策：approval 之后在 PR 里提问，owner 用固定命令在 60 秒内表态。
//
//   prod-sign:y / prod-sign:yes / y / yes  → repo（仓库正式签名）
//   prod-sign:n / prod-sign:no  / n / no   → ephemeral（一次性签名）
//   超时或其它写法                          → ephemeral
//
// 只认仓库 owner 的、晚于提问评论的会话评论；命令必须独占一行（解析规则见
// ./pr-signing-command.mjs）。结果写到 $GITHUB_OUTPUT 的 signing。
//
// 环境变量：GH_TOKEN、GH_REPO（owner/repo）、PR_NUMBER，
//           GH_OWNER（可选，默认取 GH_REPO 的 owner）、
//           PR_SIGNING_TIMEOUT_SECONDS（可选，默认 60）。

import { appendFile } from "node:fs/promises";
import { parseSignCommand } from "./pr-signing-command.mjs";

const env = process.env;
const REPO = required("GH_REPO", env.GH_REPO || env.GITHUB_REPOSITORY);
const PR_NUMBER = required("PR_NUMBER", env.PR_NUMBER);
const OWNER = env.GH_OWNER || REPO.split("/")[0];
const TIMEOUT_SECONDS = Number(env.PR_SIGNING_TIMEOUT_SECONDS || 60);
const INTERVAL_MS = 5000;
const API = "https://api.github.com";

function required(name, value) {
  if (!value) throw new Error(`缺少环境变量 ${name}`);
  return value;
}

async function gh(path, init = {}) {
  const response = await fetch(`${API}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${required("GH_TOKEN", env.GH_TOKEN)}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      ...init.headers,
    },
  });
  if (!response.ok) {
    throw new Error(`GitHub ${path} 请求失败 (${response.status}): ${await response.text()}`);
  }
  return response.status === 204 ? null : response.json();
}

async function listComments(since) {
  const comments = [];
  for (let page = 1; ; page += 1) {
    const batch = await gh(
      `/repos/${REPO}/issues/${PR_NUMBER}/comments?per_page=100&page=${page}&since=${encodeURIComponent(since)}`,
    );
    comments.push(...batch);
    if (batch.length < 100) break;
  }
  return comments;
}

function decide(comments, since) {
  const ordered = comments
    .filter((comment) => comment.user?.login === OWNER && comment.created_at > since)
    .sort((left, right) => left.created_at.localeCompare(right.created_at));
  for (const comment of ordered) {
    const sign = parseSignCommand(comment.body);
    if (sign === "y") return "repo";
    if (sign === "n") return "ephemeral";
  }
  return null;
}

async function report(signing) {
  if (env.GITHUB_OUTPUT) await appendFile(env.GITHUB_OUTPUT, `signing=${signing}\n`);
  console.log(`[pr-signing] ${REPO}#${PR_NUMBER} 签名方式：${signing}`);
}

async function main() {
  const question = [
    `@${OWNER} 本次 PR 构建是否使用仓库正式签名？`,
    "",
    "- 回复 `prod-sign:y`（或 `y`）→ 使用仓库正式签名",
    "- 回复 `prod-sign:n`（或 `n`）→ 使用一次性签名",
    "",
    `请在 ${TIMEOUT_SECONDS} 秒内回复，超时按一次性签名处理。`,
    "只有仓库 owner 的独立一行回复有效。",
  ].join("\n");
  const prompt = await gh(`/repos/${REPO}/issues/${PR_NUMBER}/comments`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ body: question }),
  });
  const since = prompt.created_at;

  const attempts = Math.ceil((TIMEOUT_SECONDS * 1000) / INTERVAL_MS);
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, INTERVAL_MS));
    const decision = decide(await listComments(since), since);
    if (decision) {
      await report(decision);
      return;
    }
  }
  await report("ephemeral");
}

main().catch((error) => {
  console.error("[pr-signing] 失败：", error);
  process.exit(1);
});
