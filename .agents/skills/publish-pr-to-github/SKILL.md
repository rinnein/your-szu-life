---
name: publish-pr-to-github
description: >
    把 CNB 上已经 review 完成的合并请求（或指定 CNB 分支）发布成 GitHub 上的 PR。
    GitHub 的 main 受保护，因此永远推 cnb/<源分支> 再由 API 建 PR。使用场景：
    「把这个 PR 发到 GitHub」「同步到 GitHub 开个 PR」「push this branch to GitHub and open a PR」。
argument-hint: "[源分支，可选，默认取当前 PR 的源分支]"
---

# 把 CNB 的定稿 PR 发到 GitHub

仓库：CNB `rinnein/szu/your-szu-life` → GitHub `rinnein/your-szu-life`。

发布动作已经封装好，不要自己拼 `git push`：

- 脚本：`.ci/push-to-github-pr.mjs`（推 `cnb/<源分支>` + 建 PR，见下文）
- 流水线事件：`api_trigger_github_publish`（`.cnb.yml` 的 `"**"` 下，单命令等价物）

脚本行为要点（据此判断是否正常）：

- 只推 `cnb/<源分支>`。`main` 受保护，永远不会被直接推送。
- 推送前比较 `github/<目标分支>..<源分支>`，为 0 个提交就打印「没有需要同步的提交」并直接退出，不算失败。
- 同一个 `cnb/<源分支>` 的 PR **已存在就更新**：先 force push 分支（PR 的 diff 随之更新），
  再 PATCH 标题与正文（正文里是本次的提交列表与构建链接），不会新建第二个 PR。
- 目标 PR 已被关闭/合并时才会新建一个；人工改过的标题（不以 `chore: sync ` 开头）不会被覆盖。
- 通过 credential helper 传 token，不会把 token 写进 URL 或日志。

## 1. 先确定两个分支

1. 源分支：用户明确指定 > `$CNB_PULL_REQUEST_BRANCH`（PR 上下文）> 当前分支。
2. 目标分支：默认 `main`；用户指定其它分支时按用户说的填。

分支没确定就不要执行，先问。

## 2. 有 node 和 git：直接跑脚本（首选，能立刻拿到 PR 链接）

`node -v` 与 `git --version` 都能输出版本时走这条。这一步不需要「替我上班」，
推 GitHub 用的是 `GITHUB_SYNC_TOKEN`，读 CNB 分支只需要只读权限。

```bash
# 2.1 先 dry-run，确认提交数和提交内容符合预期
source_branch=<源分支> base_branch=main DRY_RUN=true node .ci/push-to-github-pr.mjs

# 2.2 计划没问题再正式执行
source_branch=<源分支> base_branch=main node .ci/push-to-github-pr.mjs
```

可选的 `pr_title=<标题>` 覆盖默认标题。正式执行时脚本会打印 `已创建 PR #N: <链接>` 或 `已更新已有 PR #N: <链接>`。

## 3. 没有 node：用 OpenAPI 触发同一条流水线

```bash
curl -sS -X POST \
  -H "Authorization: Bearer $CNB_TOKEN" \
  -H "Accept: application/vnd.cnb.api+json" \
  -H "Content-Type: application/json" \
  -d '{"event":"api_trigger_github_publish","branch":"<源分支>","env":{"base_branch":"main"}}' \
  "$CNB_API_ENDPOINT/$CNB_REPO_SLUG/-/build/start"
```

响应里取 `sn`（构建号）和构建链接。这条流水线跑在 `node:lts`（当前 v26）里，结果要看构建日志最后一屏；
能取到 GitHub PR 链接就贴链接，取不到就贴构建链接并说明「结果在构建日志里」。

## 4. 汇报（必须做）

在触发你的那个 CNB PR/Issue 下评论，包含：

- 源分支 → 目标分支
- 提交数
- GitHub PR 链接（拿不到就给构建链接）
- 结果：成功 / 跳过（无新提交）/ 失败及原因

## 5. 红线

- 不直接 `git push` 到 GitHub 的 `main` 或任何非 `cnb/` 前缀分支。
- 不 `--mirror`、不推 tag、不 `reset --hard`。
- 不把 `GITHUB_SYNC_TOKEN` 写进命令行 URL、提交、文件或评论；命令里只用 `$GITHUB_SYNC_TOKEN` 引用。
- 不为了让流水线通过而改 `.cnb.yml` 里的 token 引用或跳过检查。
- 同一请求失败最多重试一次；仍失败就如实报告错误原文，不要静默改方案。

## 6. 常见失败

| 现象 | 原因 / 处理 |
|---|---|
| `创建 PR 失败: 422 No commits between …` | 源分支与目标分支没有差异，或目标分支名写错 |
| `创建 PR 失败: 403` | PAT 缺 `Pull requests: RW`，或账号对该仓库没有写权限 |
| `refusing to allow a Personal Access Token to create or update workflow` | PAT 缺 `Workflows: RW`，见仓库配置说明 |
| `没有需要同步的提交` | 正常跳过，不是错误 |
| `无法确定源分支` | 没传 `source_branch`，且当前不是 PR 上下文 |
