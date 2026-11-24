import { useEffect, useRef, useState, type FormEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, LoadingIndicator, TextField } from "material/react";
import { Banner, bindField, toast } from "../ui";
import { errorMessage } from "../async-state";
import { usePending } from "../use-pending";
import { authFailureFor, beginAuthentication, useAuth } from "../auth";
import { ProfileForm } from "../ProfileForm";
import { useRegisterNextAction, type StepNextAction } from "../step-next-action";
import {
  CAMPUS_CARD_LENGTH,
  campusCardDigits,
  campusCardError,
  isCampusCardComplete,
} from "../campus-card";
import type { StepProps } from "./step-types";
import { authStepView, isTerminalAuthFailure, type AutoAttempt } from "./auth-view";
import { beginSetupAuthentication, cancelAuthentication, consumeAuthFailureNotice } from "./api";

export function AuthStep({ onComplete, registerNext, reauth }: StepProps) {
  const { state, profile, userData } = useAuth();
  const [campusCard, setCampusCard] = useState(profile?.campusCardNumber ?? "");
  const [password, setPassword] = useState("");
  const [cardSubmitted, setCardSubmitted] = useState(false);
  const [manual, setManual] = useState(false);
  const [autoAttempt, setAutoAttempt] = useState<AutoAttempt>("idle");
  const [profileWaitExpired, setProfileWaitExpired] = useState(false);
  const lastFailure = useRef<string | undefined>(undefined);
  const pending = usePending();
  const submitPending = pending.isPending("submit");

  useEffect(() => {
    void consumeAuthFailureNotice()
      .then((notice) => {
        if (notice) toast.danger("统一认证未完成", { description: notice });
      })
      .catch(() => undefined);
  }, []);

  // 终态无法靠静默重登修复，用 Snackbar 说明原因；其余失败先静默恢复。
  useEffect(() => {
    if (!isTerminalAuthFailure(state)) return;
    const key = Object.keys(state)[0] ?? "unknown";
    if (lastFailure.current === key) return;
    lastFailure.current = key;
    const failure = authFailureFor(state);
    if (failure) toast.danger(failure.title, { description: failure.detail });
  }, [state]);

  // 已保存的校园卡号可能在 AuthProvider 完成读取后才到达。
  useEffect(() => {
    if (!campusCard && profile?.campusCardNumber) {
      setCampusCard(profile.campusCardNumber);
    }
  }, [campusCard, profile?.campusCardNumber]);

  // 资料读取是本地操作，通常很快；超时后仍没有资料就退回手动输入。
  useEffect(() => {
    if (!reauth || profile) return;
    const timer = window.setTimeout(() => setProfileWaitExpired(true), 1500);
    return () => window.clearTimeout(timer);
  }, [reauth, profile]);

  // 登录失效后静默优先：有已保存密码就自动登录，完全失败才展示表单。
  useEffect(() => {
    if (!reauth || autoAttempt !== "idle") return;
    if ("Authenticated" in state || "Authenticating" in state) return;
    // 这些终态无法通过重新登录在本机自动修复，直接交给用户处理。
    if (isTerminalAuthFailure(state)) {
      setAutoAttempt("failed");
      return;
    }
    if (profile === undefined) {
      // Unauthenticated / AwaitingProfile 表示本机没有可用于自动登录的完整资料。
      if ("Unauthenticated" in state || "AwaitingProfile" in state) {
        setAutoAttempt("failed");
        return;
      }
      if (!profileWaitExpired) return;
      setAutoAttempt("failed");
      return;
    }
    if (!profile.hasPassword) {
      setAutoAttempt("failed");
      return;
    }
    setAutoAttempt("running");
    lastFailure.current = undefined;
    void beginAuthentication().catch((cause) => {
      setAutoAttempt("failed");
      toast.danger("无法开始统一认证", { description: errorMessage(cause) });
    });
  }, [reauth, autoAttempt, state, profile, profileWaitExpired]);

  useEffect(() => {
    let active = true;
    const stops: Array<() => void> = [];
    const register = async () => {
      const offFailure = await listen("auth-login-failed", () => {
        setAutoAttempt("failed");
        void cancelAuthentication().catch(() => undefined);
        toast.danger("登录失败，请检查校园卡号和密码后重试。");
      });
      if (active) stops.push(offFailure);
      else offFailure();
      const offManual = await listen("auth-manual-required", () => setManual(true));
      if (active) stops.push(offManual);
      else offManual();
    };
    void register();
    return () => {
      active = false;
      stops.forEach((off) => off());
    };
  }, []);

  // 键盘回车与底栏按钮走同一个入口；重复提交由 usePending 的同键拦截挡住。
  const submitCredentials = async () => {
    if (!isCampusCardComplete(campusCard)) {
      setCardSubmitted(true);
      return;
    }
    if (!password) {
      toast.danger("密码不能为空");
      return;
    }
    setManual(false);
    lastFailure.current = undefined;
    const outcome = await pending.run("submit", () =>
      beginSetupAuthentication(campusCard.trim(), password),
    );
    if (!outcome.ok && !outcome.skipped) {
      toast.danger("无法开始统一认证", { description: errorMessage(outcome.error) });
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void submitCredentials();
  };

  const cardError = campusCardError(campusCard, cardSubmitted);

  const view = authStepView(state, { reauth, autoAttempt });

  let nextAction: StepNextAction | null = null;
  if (view.kind === "authenticated") {
    nextAction = { label: "继续", run: onComplete };
  } else if (view.kind === "credentials") {
    nextAction = {
      label: "登录并继续",
      disabled: submitPending,
      busy: submitPending,
      run: submitCredentials,
    };
  }
  // AwaitingProfile 时把主操作让给 ProfileForm 的注册。
  useRegisterNextAction(registerNext, nextAction, view.kind !== "awaitingProfile");

  switch (view.kind) {
    case "authenticated":
      return (
        <Card variant="outlined">
          <div slot="headline">账户已就绪</div>
          <div slot="subhead">学号由统一认证自动获取，账号资料已安全保存到本机凭据存储。</div>
          <dl className="detail-list">
            <dt>姓名</dt>
            <dd>{profile?.name ?? "未获取"}</dd>
            <dt>学号</dt>
            <dd>{profile?.studentId ?? "未获取"}</dd>
            <dt>校园卡号</dt>
            <dd>{profile?.campusCardNumber ?? "未获取"}</dd>
          </dl>
        </Card>
      );

    case "authenticating":
      return (
        <Card variant="outlined">
          <div slot="headline">正在完成统一认证</div>
          <div slot="subhead">请稍候，认证完成后会自动进入下一步。</div>
          <div className="grid gap-3">
            <p className="status-line" role="status">
              <LoadingIndicator size={20} aria-hidden="true" />
              等待官方认证完成…
            </p>
            {manual && (
              <Banner status="warning">
                <Banner.Title>需要在官方页面手动登录</Banner.Title>
                <Banner.Description>
                  请在打开的官方页面完成登录，完成后会自动继续；也可以返回重试。
                </Banner.Description>
              </Banner>
            )}
            <div>
              <Button
                variant="text"
                onClick={() => {
                  setAutoAttempt("failed");
                  setManual(false);
                  void cancelAuthentication().catch(() => undefined);
                }}
              >
                取消并重试
              </Button>
            </div>
          </div>
        </Card>
      );

    case "awaitingProfile":
      return (
        <ProfileForm
          onboarding
          registerNext={registerNext}
          onlyCampusCard
          name={view.name ?? userData?.name}
          initial={{ studentId: view.studentId || userData?.studentId || "", hasPassword: true }}
          onSaved={() => onComplete()}
        />
      );

    case "restoring":
      return (
        <Card variant="outlined">
          <div slot="headline">正在恢复统一认证</div>
          <div slot="subhead">正在使用本机保存的凭据自动登录；无法自动恢复时才需要你操作。</div>
          <div className="grid gap-3">
            <p className="status-line" role="status">
              <LoadingIndicator size={20} aria-hidden="true" />
              正在恢复统一认证…
            </p>
            <div>
              <Button
                variant="text"
                onClick={() => {
                  setAutoAttempt("failed");
                  void cancelAuthentication().catch(() => undefined);
                }}
              >
                手动登录
              </Button>
            </div>
          </div>
        </Card>
      );

    case "credentials":
      return (
        <Card variant="outlined">
          <div slot="headline">账户与个人信息</div>
          <div slot="subhead">
            输入校园卡号和登录密码后会自动打开深圳大学统一认证；学号由认证结果自动获取。
          </div>
          <form className="settings-form" onSubmit={onSubmit}>
            <TextField
              label="校园卡号"
              type="tel"
              inputmode="numeric"
              pattern="[0-9]*"
              maxlength={CAMPUS_CARD_LENGTH}
              value={campusCard}
              required
              disabled={submitPending}
              error={cardError !== undefined}
              supportingText={cardError ?? "6 位数字"}
              {...bindField((value) => setCampusCard(campusCardDigits(value)))}
            />
            <TextField
              label="登录密码"
              type="password"
              value={password}
              required
              disabled={submitPending}
              autocomplete="current-password"
              {...bindField(setPassword)}
            />
          </form>
        </Card>
      );
  }
}
