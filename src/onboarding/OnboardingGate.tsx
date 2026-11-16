import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { Card, LoadingIndicator } from "material/react";
import { toast } from "../ui";
import { useAuth, type AuthState } from "../auth";
import { PageSurface } from "../shell/PageSurface";
import { getOnboardingState, type OnboardingState } from "./api";
import { OnboardingWizard } from "./OnboardingWizard";

function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * 认证状态的稳定标签。用对象本身做依赖会在 refresh 后反复触发。
 */
function authStateKey(state: AuthState): string {
  if ("Authenticated" in state) return `authenticated:${state.Authenticated.studentId}`;
  if ("AwaitingProfile" in state) return "awaitingProfile";
  if ("Authenticating" in state) return "authenticating";
  if ("Unauthenticated" in state) return "unauthenticated";
  if ("Checking" in state) return "checking";
  return Object.keys(state)[0] ?? "unknown";
}

/**
 * 初始化设置门。
 *
 * 满足任一条件时接管整个应用内容：
 * - 当前版本仍有未完成/未跳过的初始化设置项；
 * - 统一认证未完成或已失效（此时进入认证步骤自动登录）。
 *
 * 读取设置状态失败时 fail-open，避免数据库问题把应用锁死在启动页；失败原因
 * 用一条 Snackbar 说明（浏览器预览里没有 IPC，属于预期状态，不提示）。
 *
 * 这里刻意保留「两个字段 + 一次 fetch」的朴素写法，没有换成 `Async<T>`：它是
 * 启动路径上的三态门，多出的一次渲染会改变各页面的挂载时序，而当前写法在
 * 路由级集成测试（EnrollmentPage 等）里已经稳定。需要可辨识联合的是步骤内部
 * 那类状态，不是这个门。
 */
export function OnboardingGate({ children }: { children: ReactNode }) {
  const { state } = useAuth();
  const [onboarding, setOnboarding] = useState<OnboardingState>();
  const [error, setError] = useState<string>();
  const [dismissed, setDismissed] = useState(false);
  const reported = useRef(false);

  const refresh = useCallback(async () => {
    try {
      const next = await getOnboardingState();
      // 测试环境或异常返回可能不是有效快照；fail-open，不要让应用卡在启动页。
      if (!next || !Array.isArray(next.steps)) {
        throw new Error("初始化设置状态格式无效");
      }
      setOnboarding(next);
      setError(undefined);
    } catch (cause) {
      setOnboarding(undefined);
      setError(String(cause));
      if (insideTauri() && !reported.current) {
        reported.current = true;
        toast.danger("读取初始化设置失败", { description: String(cause) });
      }
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const authKey = authStateKey(state);

  // 桌面端登录成功后本地页面会重载；Android 端通过事件更新状态。两种情况都重取一次。
  useEffect(() => {
    if (authKey.startsWith("authenticated:")) void refresh();
  }, [authKey, refresh]);

  // 调试页可以要求重新开始向导；后端把新状态随事件一起发回。
  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<OnboardingState>("onboarding-restarted", (event) => {
      if (!active) return;
      const next = event.payload;
      if (next && Array.isArray(next.steps)) {
        setDismissed(false);
        setOnboarding(next);
        setError(undefined);
      } else {
        void refresh();
      }
    })
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [refresh]);

  if (!onboarding || "Checking" in state) {
    if (error && !onboarding) return <>{children}</>;
    return (
      <PageSurface centered>
        <Card className="status-card" variant="outlined">
          <span className="status-line" role="status">
            <LoadingIndicator size={24} aria-hidden="true" />
            正在检查初始化设置…
          </span>
        </Card>
      </PageSurface>
    );
  }

  const authLost = !("Authenticated" in state);
  const needsWizard = onboarding.needsSetup || authLost;
  const canDismiss = !onboarding.needsSetup && authLost;

  if (needsWizard && !dismissed) {
    return (
      <OnboardingWizard
        state={onboarding}
        canDismiss={canDismiss}
        onDismiss={() => setDismissed(true)}
        onFinished={() => {
          setDismissed(false);
          reported.current = false;
          void refresh();
        }}
      />
    );
  }

  return <>{children}</>;
}
