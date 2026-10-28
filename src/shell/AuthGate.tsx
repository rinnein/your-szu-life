import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, Button } from "material/react";
import { Banner } from "../ui";
import { authFailureFor, beginAuthentication, useAuth, type Profile } from "../auth";
import { ProfileForm } from "../ProfileForm";
import { BrandMark } from "../components";
import { PageSurface } from "./AppShell";

/**
 * 统一认证门。
 *
 * 首页 / 学习 / 生活三个内容页共用：未完成统一认证时渲染全屏认证卡片
 * （`data-fullscreen` 会隐藏导航），认证完成后渲染 `children`。
 */
export function AuthGate({ children }: { children: ReactNode }) {
  const { state, setProfile, setState, userData } = useAuth();
  const [busy, setBusy] = useState(false);

  const beginWithBusy = async () => {
    setBusy(true);
    try {
      await beginAuthentication();
    } finally {
      setBusy(false);
    }
  };

  const retryButton = (label: string) => (
    <Button className="w-full" variant="filled" onClick={beginWithBusy} disabled={busy}>
      {busy && <LoadingIndicator size={16} />}
      {busy ? "正在打开…" : label}
    </Button>
  );

  if ("Checking" in state)
    return (
      <PageSurface centered showAbout>
        <Card className="status-card" variant="outlined">
          <div className="p-6 text-center flex flex-col items-center gap-3">
            <LoadingIndicator size={40} />
            <span className="font-semibold text-base">正在检查统一认证状态…</span>
          </div>
        </Card>
      </PageSurface>
    );

  if ("Unauthenticated" in state)
    return (
      <PageSurface centered showAbout>
        <AuthCard
          title="请完成统一认证"
          detail="请在当前窗口完成深圳大学官方登录，登录成功后会自动返回首页。"
        >
          {retryButton("打开认证窗口")}
        </AuthCard>
      </PageSurface>
    );

  if ("Authenticating" in state)
    return (
      <PageSurface centered showAbout>
        <AuthCard
          title="正在恢复统一认证"
          detail="已有账户将自动尝试登录；需要验证码或出现错误时，请在官方页面手动继续。认证成功后会自动返回。"
        >
          <div className="auth-progress" role="status">
            <LoadingIndicator size={24} />
            <span>等待官方认证完成…</span>
          </div>
        </AuthCard>
      </PageSurface>
    );

  const failure = authFailureFor(state);
  if (failure)
    return (
      <PageSurface centered showAbout>
        <AuthCard title={failure.title} detail={failure.detail} status="danger">
          {retryButton(failure.action)}
        </AuthCard>
      </PageSurface>
    );

  if ("AwaitingProfile" in state) {
    const studentId = state.AwaitingProfile.studentId ?? userData?.studentId ?? "";
    return (
      <PageSurface centered showAbout>
        <ProfileForm
          onlyCampusCard
          name={state.AwaitingProfile.name ?? userData?.name}
          initial={{ studentId, hasPassword: true }}
          onSaved={async () => {
            const saved = await invoke<Profile | null>("get_account_profile");
            setProfile(saved ?? undefined);
            if (saved)
              setState({ Authenticated: { name: saved.name, studentId: saved.studentId } });
          }}
        />
      </PageSurface>
    );
  }

  return <>{children}</>;
}

function AuthCard({
  title,
  detail,
  children,
  status,
}: {
  title: string;
  detail: string;
  children?: ReactNode;
  status?: "default" | "danger";
}) {
  return (
    <Card className="auth-card" variant="outlined">
      <div className="auth-card-content p-6 flex flex-col items-center text-center">
        <BrandMark large />
        <h2 className="text-xl font-bold mt-3 text-foreground">{title}</h2>
        <p className="text-sm text-muted mt-1 max-w-sm">{detail}</p>
        {status === "danger" && (
          <Banner status="danger" className="auth-alert my-3 w-full text-left">
            <Banner.Title>需要重新处理</Banner.Title>
            <Banner.Description>请确认当前网络和官方认证窗口状态后再试。</Banner.Description>
          </Banner>
        )}
        <div className="mt-4 w-full">{children}</div>
      </div>
    </Card>
  );
}
