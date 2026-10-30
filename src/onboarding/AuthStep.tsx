import { useEffect, useRef, useState, type FormEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, LoadingIndicator, TextField } from "material/react";
import { Banner, bindField, toast } from "../ui";
import { authFailureFor, beginAuthentication, useAuth, type AuthState } from "../auth";
import { ProfileForm } from "../ProfileForm";
import { useRegisterNextAction, type StepNextAction } from "../step-next-action";
import type { StepProps } from "./step-types";
import { beginSetupAuthentication, cancelAuthentication, consumeAuthFailureNotice } from "./api";

type AutoAttempt = "idle" | "running" | "failed";

/** 无法在本机通过静默重新登录修复的状态。 */
function isTerminalAuthFailure(state: AuthState): boolean {
  return (
    "WebViewUpdateRequired" in state ||
    "AccountMismatch" in state ||
    "CredentialStorageFailed" in state ||
    "CredentialCaptureFailed" in state ||
    "SessionCaptureFailed" in state
  );
}

export function AuthStep({ onComplete, registerNext, reauth }: StepProps) {
  const { state, profile, userData } = useAuth();
  const [campusCard, setCampusCard] = useState(profile?.campusCardNumber ?? "");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [manual, setManual] = useState(false);
  const [autoAttempt, setAutoAttempt] = useState<AutoAttempt>("idle");
  const [profileWaitExpired, setProfileWaitExpired] = useState(false);
  const lastFailure = useRef<string | undefined>(undefined);

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
    setBusy(true);
    void beginAuthentication()
      .catch((cause) => {
        setAutoAttempt("failed");
        toast.danger("无法开始统一认证", { description: String(cause) });
      })
      .finally(() => setBusy(false));
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

  const submitCredentials = async () => {
    if (!/^\d{6}$/.test(campusCard.trim())) {
      toast.danger("校园卡号必须是 6 位数字");
      return;
    }
    if (!password) {
      toast.danger("密码不能为空");
      return;
    }
    setBusy(true);
    setManual(false);
    lastFailure.current = undefined;
    try {
      await beginSetupAuthentication(campusCard.trim(), password);
    } catch (cause) {
      toast.danger("无法开始统一认证", { description: String(cause) });
    } finally {
      setBusy(false);
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void submitCredentials();
  };

  const awaitingProfile = "AwaitingProfile" in state;
  let nextAction: StepNextAction | null = null;
  if ("Authenticated" in state) {
    nextAction = { label: "继续", run: onComplete };
  } else if (!awaitingProfile && !("Authenticating" in state)) {
    if (!reauth || autoAttempt === "failed") {
      nextAction = {
        label: "登录并继续",
        disabled: busy,
        busy,
        run: submitCredentials,
      };
    }
  }

  // AwaitingProfile 时把主操作让给 ProfileForm 的注册。
  useRegisterNextAction(registerNext, nextAction, !awaitingProfile);

  if ("Authenticated" in state) {
    return (
      <Card variant="outlined">
        <div slot="headline">账户已就绪</div>
        <div slot="subhead">
          学号由统一认证自动获取；学号、校园卡号和密码已安全保存到本机凭据存储。
        </div>
        <div className="p-4 grid gap-3">
          <div className="grid gap-1 text-sm text-foreground">
            <span>姓名：{profile?.name ?? "未获取"}</span>
            <span>学号：{profile?.studentId ?? "未获取"}</span>
            <span>校园卡号：{profile?.campusCardNumber ?? "未获取"}</span>
          </div>
        </div>
      </Card>
    );
  }

  if ("Authenticating" in state) {
    return (
      <Card variant="outlined">
        <div slot="headline">正在完成统一认证</div>
        <div slot="subhead">
          二维码登录已禁用，正在后台刷新会话或使用上一步的凭据自动登录，不需要在官方页面再次点击登录。
        </div>
        <div className="p-4 grid gap-3">
          <div className="auth-progress" role="status">
            <LoadingIndicator size={24} />
            <span>等待官方认证完成…</span>
          </div>
          {manual && (
            <Banner status="warning">
              <Banner.Title>自动登录已切换为手动</Banner.Title>
              <Banner.Description>
                请在官方页面完成登录；完成后会自动继续。也可以返回重试。
              </Banner.Description>
            </Banner>
          )}
          <Button
            variant="outlined"
            onClick={() => {
              setAutoAttempt("failed");
              setManual(false);
              void cancelAuthentication().catch(() => undefined);
            }}
          >
            取消并重试
          </Button>
        </div>
      </Card>
    );
  }

  if (awaitingProfile) {
    const studentId = state.AwaitingProfile.studentId ?? userData?.studentId ?? "";
    return (
      <ProfileForm
        onboarding
        registerNext={registerNext}
        onlyCampusCard
        name={state.AwaitingProfile.name ?? userData?.name}
        initial={{ studentId, hasPassword: true }}
        onSaved={() => onComplete()}
      />
    );
  }

  if (reauth && autoAttempt !== "failed") {
    return (
      <Card variant="outlined">
        <div slot="headline">正在恢复统一认证</div>
        <div slot="subhead">
          正在静默刷新会话或使用已保存的凭据自动登录；只有在无法自动恢复时才会要求你手动操作。
        </div>
        <div className="p-4 grid gap-3">
          <div className="auth-progress" role="status">
            <LoadingIndicator size={24} />
            <span>正在恢复统一认证…</span>
          </div>
          <Button
            variant="text"
            disabled={busy}
            onClick={() => {
              setAutoAttempt("failed");
              void cancelAuthentication().catch(() => undefined);
            }}
          >
            手动登录
          </Button>
        </div>
      </Card>
    );
  }

  return (
    <Card variant="outlined">
      <div slot="headline">账户与个人信息</div>
      <div slot="subhead">
        输入校园卡号和登录密码后，将自动打开深圳大学统一认证并完成登录；学号会自动获取，无需手动填写。
      </div>
      <div className="p-4">
        <form className="settings-form" onSubmit={onSubmit}>
          <TextField
            label="校园卡号"
            value={campusCard}
            inputMode="numeric"
            required
            disabled={busy}
            supportingText="6 位数字"
            {...bindField(setCampusCard)}
          />
          <TextField
            label="登录密码"
            type="password"
            value={password}
            required
            disabled={busy}
            autocomplete="current-password"
            {...bindField(setPassword)}
          />
        </form>
      </div>
    </Card>
  );
}
