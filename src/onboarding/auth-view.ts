import type { AuthState } from "../auth";

/** 自动重认证的进度：idle（还没试）/ running（已发起）/ failed（需要用户接手）。 */
export type AutoAttempt = "idle" | "running" | "failed";

/**
 * 认证步骤只有一张卡片，但以前由 `busy` / `manual` / `autoAttempt` /
 * `profileWaitExpired` / 认证状态五个维度共同决定渲染哪一个分支，分支之间还
 * 出现过「正在自动登录但 busy 已清掉」这类可表示的矛盾组合。这里把渲染决策
 * 收成一个可辨识联合，渲染处穷尽 switch，状态组合由类型本身排除。
 */
export type AuthStepView =
  | { kind: "authenticated" }
  | { kind: "authenticating" }
  | { kind: "awaitingProfile"; studentId: string; name?: string }
  | { kind: "restoring" }
  | { kind: "credentials" };

/** 无法在本机通过静默重新登录修复的状态。 */
export function isTerminalAuthFailure(state: AuthState): boolean {
  return (
    "WebViewUpdateRequired" in state ||
    "AccountMismatch" in state ||
    "CredentialStorageFailed" in state ||
    "CredentialCaptureFailed" in state ||
    "SessionCaptureFailed" in state
  );
}

export function authStepView(
  state: AuthState,
  options: { reauth?: boolean; autoAttempt: AutoAttempt },
): AuthStepView {
  if ("Authenticated" in state) return { kind: "authenticated" };
  if ("Authenticating" in state) return { kind: "authenticating" };
  if ("AwaitingProfile" in state) {
    const { studentId, name } = state.AwaitingProfile;
    return { kind: "awaitingProfile", studentId: studentId ?? "", name: name ?? undefined };
  }
  // 重认证期间先静默尝试；试过且失败，或用户选择手动后走凭据表单。
  if (options.reauth && options.autoAttempt !== "failed") return { kind: "restoring" };
  return { kind: "credentials" };
}
