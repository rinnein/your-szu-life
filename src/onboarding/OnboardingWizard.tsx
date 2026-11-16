import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button, Card, LoadingIndicator, Progress } from "material/react";
import { toast } from "../ui";
import { errorMessage } from "../async-state";
import { usePending } from "../use-pending";
import { BrandMark } from "../components";
import { PageSurface } from "../shell/PageSurface";
import { useDemoModeActivation } from "../shell/use-demo-mode";
import { useAuth } from "../auth";
import type { StepNextAction } from "../step-next-action";
import { completeOnboardingStep, skipOnboardingStep, type OnboardingState } from "./api";
import { prewarmSsoModules } from "./prewarm";
import { STEP_META } from "./registry";
import { createWizardState, wizardReducer } from "./wizard-machine";

export function OnboardingWizard({
  state: initialState,
  canDismiss,
  onDismiss,
  onFinished,
}: {
  state: OnboardingState;
  canDismiss: boolean;
  onDismiss: () => void;
  onFinished: () => void;
}) {
  const { state: authState } = useAuth();
  const handleBrandTap = useDemoModeActivation();
  const signedIn = "Authenticated" in authState;

  // 步骤计划、当前索引与「已完成」的推进全部收敛在 reducer 里：同步步骤让后端
  // 顺带完成后续步骤、以及中途会话失效插回认证，改的都是同一份状态。
  const [wizard, dispatch] = useReducer(wizardReducer, undefined, () =>
    createWizardState(initialState, signedIn),
  );

  // 「正在保存 / 正在执行」不属于导航状态：按操作键登记，渲染时派生。
  const pending = usePending();
  const [nextAction, setNextAction] = useState<StepNextAction | null>(null);
  const registerNext = useCallback((action: StepNextAction | null) => setNextAction(action), []);

  const finished = useRef(false);
  const finish = useCallback(() => {
    if (finished.current) return;
    finished.current = true;
    onFinished();
  }, [onFinished]);

  useEffect(() => {
    if (wizard.finished) finish();
  }, [wizard.finished, finish]);

  // 启动时的后台刷新可能在使用向导期间发现会话失效；把静默重认证步骤插到最前面，
  // 避免用户继续做设置却漏掉已经失效的登录。恢复后重认证步骤自动前进。
  useEffect(() => {
    dispatch({ type: signedIn ? "sessionRestored" : "sessionLost" });
  }, [signedIn]);

  const current = wizard.plan[wizard.index];
  const definition = current ? wizard.steps.find((step) => step.id === current.id) : undefined;
  const meta = current ? STEP_META[current.id] : undefined;

  /**
   * 完成当前步骤。后端会返回新状态：同步步骤可能已经把它能带来的宿舍与 AI
   * 设置标为完成，这些步骤随后不会再出现在计划里。
   */
  const complete = async () => {
    if (!current) return;
    if (current.reauth) {
      dispatch({ type: "sessionRestored" });
      return;
    }
    const outcome = await pending.run(`complete:${current.id}`, () =>
      completeOnboardingStep(current.id),
    );
    if (outcome.ok) dispatch({ type: "serverState", state: outcome.value });
    else if (!outcome.skipped) {
      toast.danger("保存初始化设置进度失败", { description: errorMessage(outcome.error) });
    }
  };

  const skip = async () => {
    if (!current) return;
    const outcome = await pending.run(`skip:${current.id}`, () => skipOnboardingStep(current.id));
    if (outcome.ok) dispatch({ type: "serverState", state: outcome.value });
    else if (!outcome.skipped) {
      toast.danger("跳过设置项失败", { description: errorMessage(outcome.error) });
    }
  };

  // 步骤自己负责提示可预期的失败；这里兜住的是漏网的编程错误——以前只写
  // console，用户点了没反应也不知道为什么。
  const runNext = async () => {
    if (!nextAction) return;
    const action = nextAction;
    const outcome = await pending.run("next", async () => action.run());
    if (!outcome.ok && !outcome.skipped) {
      toast.danger("这一步没有完成", { description: errorMessage(outcome.error) });
    }
  };

  // 统一认证完成后静默预热只依赖 authserver SSO 的畅课/巡课：用户还要走完剩下的
  // 设置步骤，会话可以在这段时间里建好，进入首页时待办已包含畅课项。
  // 以学号为键去重，向导中途退出重进（挂载时已登录）时也补跑一次。
  const prewarmed = useRef<string | null>(null);
  const authenticatedStudentId =
    "Authenticated" in authState ? authState.Authenticated.studentId : null;
  useEffect(() => {
    if (!authenticatedStudentId || prewarmed.current === authenticatedStudentId) return;
    prewarmed.current = authenticatedStudentId;
    prewarmSsoModules();
  }, [authenticatedStudentId]);

  // 底栏高度实测后写回 body：按钮在窄屏换行时 Snackbar 的抬升量跟着变，
  // 不再依赖一个写死的像素值。
  const actionsRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = actionsRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      document.body.style.setProperty(
        "--onboarding-actions-height",
        `${node.getBoundingClientRect().height}px`,
      );
    });
    observer.observe(node);
    return () => {
      observer.disconnect();
      document.body.style.removeProperty("--onboarding-actions-height");
    };
  }, []);

  if (!current || !meta) {
    return (
      <PageSurface centered maxWidth="md">
        <Card variant="outlined">
          <div slot="headline">初始化设置</div>
          <div slot="subhead">当前应用版本无法识别剩余的设置项，请更新应用后重试。</div>
          <div className="grid gap-3">
            <Button variant="filled" onClick={finish}>
              进入应用
            </Button>
          </div>
        </Card>
      </PageSurface>
    );
  }

  const Step = meta.component;
  const savingStep = pending.isPending(`complete:${current.id}`, `skip:${current.id}`);
  const nextBusy = pending.isPending("next") || Boolean(nextAction?.busy);
  const footerBusy = savingStep || nextBusy;

  return (
    <>
      <PageSurface fullscreen maxWidth="md">
        <div className="onboarding-wizard">
          <header className="onboarding-header">
            <div className="onboarding-header-row">
              {current.id === "welcome" ? null : (
                <button
                  type="button"
                  className="brand-mark-trigger"
                  aria-label="应用图标，连续点击 5 次进入演示模式"
                  onClick={handleBrandTap}
                >
                  <BrandMark />
                </button>
              )}
              <h1 className="onboarding-header-title">初始化设置</h1>
              <span className="onboarding-header-step">
                第 {wizard.index + 1} / {wizard.plan.length} 步
              </span>
            </div>
            <Progress
              variant="linear"
              value={wizard.index + 1}
              max={wizard.plan.length}
              aria-label="初始化设置进度"
              className="w-full"
            />
          </header>

          <Step
            key={current.id}
            onComplete={complete}
            onSkip={skip}
            reauth={current.reauth}
            registerNext={registerNext}
          />
        </div>
      </PageSurface>

      {typeof document !== "undefined" &&
        createPortal(
          <div className="onboarding-actions" ref={actionsRef}>
            <div className="onboarding-actions-inner">
              <div className="onboarding-actions-group">
                <Button
                  variant="text"
                  disabled={footerBusy || wizard.index === 0}
                  onClick={() => dispatch({ type: "back" })}
                >
                  上一步
                </Button>
                {canDismiss && (
                  <Button variant="text" disabled={footerBusy} onClick={onDismiss}>
                    稍后处理
                  </Button>
                )}
              </div>
              <div className="onboarding-actions-group">
                {definition?.skippable && (
                  <Button variant="text" disabled={footerBusy} onClick={() => void skip()}>
                    跳过
                  </Button>
                )}
                {nextAction && (
                  <Button
                    variant="filled"
                    disabled={footerBusy || nextAction.disabled}
                    onClick={() => void runNext()}
                  >
                    {nextBusy && <LoadingIndicator size={16} />}
                    {nextAction.label}
                  </Button>
                )}
              </div>
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}
