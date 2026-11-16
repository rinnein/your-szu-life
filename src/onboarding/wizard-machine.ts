import type { OnboardingState, OnboardingStep } from "./api";

/**
 * 初始化向导的导航状态。
 *
 * 之前计划表、当前索引与「完成/跳过」的推进逻辑分散在 `applyState`、两个
 * effect 和 `advance` 里：同步步骤让后端把后面的步骤自动标成已完成时、以及
 * 中途会话失效插回认证步骤时，两处都会改 `index`，谁先谁后取决于 effect 的
 * 执行顺序，`plan` 也可能短暂地与 `steps` 不一致。这里把三个字段收进一个
 * 纯函数 reducer，所有改写都经过同一处 `advance`，顺序不再依赖渲染时机。
 *
 * 「正在保存 / 正在执行」不属于导航状态：它由 `usePending` 按键表达，渲染时
 * 派生，因此这里没有 busy 旗子。
 */
export type WizardStep = {
  id: string;
  /** 由会话失效插入的重认证步骤：不写初始化进度，登录成功后自动前进。 */
  reauth?: boolean;
};

export type WizardState = {
  /** 后端返回的完整步骤表，用于查 skippable 等定义。 */
  steps: OnboardingStep[];
  /** 还没走完的步骤，按顺序。 */
  plan: WizardStep[];
  index: number;
  /** 走到末尾后置位；向导由门组件卸载，这里只保证 `onFinished` 只触发一次。 */
  finished: boolean;
};

export type WizardEvent =
  /** 完成/跳过拿到的新状态：后端可能顺带把后面的步骤标为已完成。 */
  | { type: "serverState"; state: OnboardingState }
  /** 使用过程中发现会话失效：插回（或退回）认证步骤。 */
  | { type: "sessionLost" }
  /** 会话已恢复：重认证步骤不等待用户操作，自动前进。 */
  | { type: "sessionRestored" }
  | { type: "back" };

export function buildPlan(state: OnboardingState, signedIn: boolean): WizardStep[] {
  const plan: WizardStep[] = state.steps
    .filter((step) => step.status === "pending")
    .map((step) => ({ id: step.id }));
  if (!signedIn && !plan.some((step) => step.id === "auth")) {
    plan.unshift({ id: "auth", reauth: true });
  }
  return plan;
}

export function createWizardState(state: OnboardingState, signedIn: boolean): WizardState {
  return { steps: state.steps, plan: buildPlan(state, signedIn), index: 0, finished: false };
}

function advance(state: WizardState): WizardState {
  if (state.index + 1 >= state.plan.length) return { ...state, finished: true };
  return { ...state, index: state.index + 1 };
}

export function wizardReducer(state: WizardState, event: WizardEvent): WizardState {
  switch (event.type) {
    case "serverState": {
      const next = event.state;
      const satisfied = new Set(
        next.steps.filter((step) => step.status !== "pending").map((step) => step.id),
      );
      // 已经走过的位置原样保留（含重认证步骤），后面已完成的步骤从计划里摘掉。
      const plan = state.plan.filter(
        (step, position) => position <= state.index || step.reauth || !satisfied.has(step.id),
      );
      return advance({ ...state, steps: next.steps, plan });
    }

    case "sessionLost": {
      const authIndex = state.plan.findIndex((step) => step.id === "auth");
      if (authIndex < 0) {
        return { ...state, plan: [{ id: "auth", reauth: true }, ...state.plan], index: 0 };
      }
      // 已经走过认证步骤但会话再次失效时，退回认证步骤。
      return state.index > authIndex ? { ...state, index: authIndex } : state;
    }

    case "sessionRestored": {
      if (!state.plan[state.index]?.reauth) return state;
      return advance(state);
    }

    case "back":
      return state.index === 0 ? state : { ...state, index: state.index - 1 };
  }
}
