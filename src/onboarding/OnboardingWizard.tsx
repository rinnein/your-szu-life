import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button, Card, LoadingIndicator, Progress } from "material/react";
import { toast } from "../ui";
import { BrandMark } from "../components";
import { PageSurface } from "../shell/PageSurface";
import { useAuth } from "../auth";
import type { StepNextAction } from "../step-next-action";
import { completeOnboardingStep, skipOnboardingStep, type OnboardingState } from "./api";
import { STEP_META } from "./registry";

type StepPlan = {
  id: string;
  reauth?: boolean;
};

function buildPlan(state: OnboardingState, authLost: boolean): StepPlan[] {
  const plan: StepPlan[] = state.steps
    .filter((step) => step.status === "pending")
    .map((step) => ({ id: step.id }));
  if (authLost && !plan.some((step) => step.id === "auth")) {
    plan.unshift({ id: "auth", reauth: true });
  }
  return plan;
}

export function OnboardingWizard({
  state,
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
  const [plan, setPlan] = useState<StepPlan[]>(() =>
    buildPlan(state, !("Authenticated" in authState)),
  );
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [running, setRunning] = useState(false);
  const [nextAction, setNextAction] = useState<StepNextAction | null>(null);
  const finished = useRef(false);
  const acting = useRef(false);

  const registerNext = useCallback((action: StepNextAction | null) => {
    setNextAction(action);
  }, []);

  const finish = () => {
    if (finished.current) return;
    finished.current = true;
    onFinished();
  };

  // 启动时的后台刷新可能在使用向导期间发现会话失效；把静默重认证步骤插到最前面，
  // 避免用户继续做设置却漏掉已经失效的登录。
  const authLost = !("Authenticated" in authState);
  useEffect(() => {
    if (!authLost) return;
    const authIndex = plan.findIndex((step) => step.id === "auth");
    if (authIndex < 0) {
      setPlan([{ id: "auth", reauth: true }, ...plan]);
      setIndex(0);
      return;
    }
    // 已经走过认证步骤但会话再次失效时，退回认证步骤。
    if (index > authIndex) setIndex(authIndex);
  }, [authLost, plan, index]);

  const current = plan[index];
  const definition = current ? state.steps.find((step) => step.id === current.id) : undefined;
  const meta = current ? STEP_META[current.id] : undefined;

  const next = () => {
    if (index + 1 >= plan.length) finish();
    else setIndex(index + 1);
  };

  const complete = async () => {
    if (!current || acting.current) return;
    acting.current = true;
    setBusy(true);
    try {
      if (!current.reauth) await completeOnboardingStep(current.id);
      next();
    } catch (cause) {
      toast.danger("保存初始化设置进度失败", { description: String(cause) });
    } finally {
      acting.current = false;
      setBusy(false);
    }
  };

  const skip = async () => {
    if (!current || acting.current) return;
    acting.current = true;
    setBusy(true);
    try {
      await skipOnboardingStep(current.id);
      next();
    } catch (cause) {
      toast.danger("跳过设置项失败", { description: String(cause) });
    } finally {
      acting.current = false;
      setBusy(false);
    }
  };

  const runNext = async () => {
    if (!nextAction || running) return;
    setRunning(true);
    try {
      await nextAction.run();
    } catch (cause) {
      console.error("初始化设置步骤执行失败", cause);
    } finally {
      setRunning(false);
    }
  };

  // 重新认证步骤不写设置状态：登录成功后直接进入下一步。
  useEffect(() => {
    if (!current?.reauth) return;
    if ("Authenticated" in authState) {
      if (index + 1 >= plan.length) finish();
      else setIndex(index + 1);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authState, current, index, plan.length]);

  if (!current || !meta) {
    return (
      <PageSurface centered maxWidth="md">
        <Card variant="outlined">
          <div slot="headline">初始化设置</div>
          <div slot="subhead">当前应用版本无法识别剩余的设置项，请更新应用后重试。</div>
          <div className="p-4">
            <Button variant="filled" onClick={finish}>
              进入应用
            </Button>
          </div>
        </Card>
      </PageSurface>
    );
  }

  const Step = meta.component;
  const footerBusy = busy || running || Boolean(nextAction?.busy);
  const nextBusy = running || Boolean(nextAction?.busy);

  return (
    <>
      <PageSurface fullscreen maxWidth="md">
        <div className="onboarding-wizard">
          <header className="flex flex-col items-center text-center gap-2">
            <BrandMark large />
            <h1 className="text-xl font-bold text-foreground">初始化设置</h1>
            <p className="text-sm text-muted">
              第 {index + 1} / {plan.length} 步 · {meta.label}
            </p>
            <Progress
              variant="linear"
              value={index + 1}
              max={plan.length}
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
          <div className="onboarding-actions">
            <div className="onboarding-actions-group">
              <Button
                variant="text"
                disabled={footerBusy || index === 0}
                onClick={() => setIndex(Math.max(0, index - 1))}
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
          </div>,
          document.body,
        )}
    </>
  );
}
