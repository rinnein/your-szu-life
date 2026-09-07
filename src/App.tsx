import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "@tanstack/react-router";
import { Alert, AlertDialog, Button, Card, Dropdown, Spinner } from "@heroui/react";
import { authFailureFor, useAuth } from "./auth";
import { ProfileForm } from "./ProfileForm";
import { BrandMark, PageShell } from "./components";
import {
  FaCalendarDays,
  FaChartLine,
  FaClipboardList,
  FaGraduationCap,
  FaWallet,
  FaWifi,
} from "react-icons/fa6";
import type { IconType } from "react-icons";

const features: ReadonlyArray<{ label: string; icon: IconType }> = [
  { label: "课表", icon: FaCalendarDays },
  { label: "选课", icon: FaClipboardList },
  { label: "培养方案检查", icon: FaGraduationCap },
  { label: "成绩查询", icon: FaChartLine },
  { label: "校园网登录", icon: FaWifi },
  { label: "网费与电费", icon: FaWallet },
];

function AuthCard({
  title,
  detail,
  children,
  status,
}: {
  title: string;
  detail: string;
  children?: React.ReactNode;
  status?: "default" | "danger";
}) {
  return (
    <Card className="auth-card" variant="default">
      <Card.Content className="auth-card-content">
        <BrandMark large />
        <Card.Title>{title}</Card.Title>
        <Card.Description>{detail}</Card.Description>
        {status === "danger" && (
          <Alert className="auth-alert" status="danger">
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Title>需要重新处理</Alert.Title>
              <Alert.Description>请确认当前网络和官方认证窗口状态后再试。</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {children}
      </Card.Content>
    </Card>
  );
}

function LogoutDialog({
  isOpen,
  isBusy,
  onOpenChange,
  onConfirm,
}: {
  isOpen: boolean;
  isBusy: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => Promise<void>;
}) {
  return (
    <AlertDialog isOpen={isOpen} onOpenChange={onOpenChange}>
      <AlertDialog.Backdrop>
        <AlertDialog.Container size="sm">
          <AlertDialog.Dialog>
            <AlertDialog.Icon status="danger" />
            <AlertDialog.Header>
              <AlertDialog.Heading>确认注销？</AlertDialog.Heading>
            </AlertDialog.Header>
            <AlertDialog.Body>
              这会清除本机认证数据，需要重新完成深圳大学统一认证。
            </AlertDialog.Body>
            <AlertDialog.Footer>
              <AlertDialog.CloseTrigger className="dialog-cancel">取消</AlertDialog.CloseTrigger>
              <Button variant="danger" isDisabled={isBusy} onPress={onConfirm}>
                {isBusy && <Spinner size="sm" color="current" />}
                {isBusy ? "注销中…" : "确认注销"}
              </Button>
            </AlertDialog.Footer>
          </AlertDialog.Dialog>
        </AlertDialog.Container>
      </AlertDialog.Backdrop>
    </AlertDialog>
  );
}

export default function App() {
  const { state, profile, setProfile, setState, userData } = useAuth();
  const navigate = useNavigate();
  const [placeholder, setPlaceholder] = useState<string>();
  const [authBusy, setAuthBusy] = useState(false);
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [logoutBusy, setLogoutBusy] = useState(false);

  const beginAuthentication = async () => {
    setAuthBusy(true);
    try {
      await invoke("begin_authentication");
    } finally {
      setAuthBusy(false);
    }
  };

  if ("Checking" in state)
    return (
      <PageShell centered>
        <Card className="status-card" variant="default">
          <Spinner size="lg" />
          <Card.Title>正在检查统一认证状态…</Card.Title>
        </Card>
      </PageShell>
    );
  if ("Unauthenticated" in state)
    return (
      <PageShell centered>
        <AuthCard
          title="请完成统一认证"
          detail="请在当前窗口完成深圳大学官方登录，登录成功后会自动返回工作台。"
        >
          <Button fullWidth variant="primary" onPress={beginAuthentication} isDisabled={authBusy}>
            {authBusy && <Spinner size="sm" color="current" />}
            {authBusy ? "正在打开…" : "打开认证窗口"}
          </Button>
        </AuthCard>
      </PageShell>
    );
  if ("Authenticating" in state)
    return (
      <PageShell centered>
        <AuthCard
          title="正在打开统一认证"
          detail="请在当前窗口完成深圳大学官方登录。验证成功且会话安全保存后，应用会自动返回后续设置或工作台。"
        >
          <div className="auth-progress" role="status">
            <Spinner />
            <span>等待官方认证完成…</span>
          </div>
        </AuthCard>
      </PageShell>
    );

  const failure = authFailureFor(state);
  if (failure)
    return (
      <PageShell centered>
        <AuthCard title={failure.title} detail={failure.detail} status="danger">
          <Button fullWidth variant="primary" onPress={beginAuthentication} isDisabled={authBusy}>
            {authBusy && <Spinner size="sm" color="current" />}
            {authBusy ? "正在打开…" : failure.action}
          </Button>
        </AuthCard>
      </PageShell>
    );
  if ("AwaitingProfile" in state) {
    const studentId = state.AwaitingProfile.studentId ?? userData?.studentId ?? "";
    return (
      <PageShell centered>
        <ProfileForm
          onlyCampusCard
          name={state.AwaitingProfile.name ?? userData?.name}
          initial={{ studentId, hasPassword: true }}
          onSaved={async () => {
            const saved = await invoke<any>("get_account_profile");
            setProfile(saved ?? undefined);
            if (saved)
              setState({ Authenticated: { name: saved.name, studentId: saved.studentId } });
          }}
        />
      </PageShell>
    );
  }

  const auth =
    "Authenticated" in state ? state.Authenticated : { studentId: profile?.studentId ?? "" };
  return (
    <PageShell>
      <header className="app-header">
        <div>
          <div className="brand-line">
            <BrandMark />
            <span className="eyebrow">YOUR SZU LIFE</span>
          </div>
          <h1>深大生活工作台</h1>
        </div>
        <Dropdown>
          <Dropdown.Trigger className="account-trigger" aria-label="打开账户菜单">
            <span>{auth.studentId}</span>
            <span className="account-chevron" aria-hidden="true">
              ⌄
            </span>
          </Dropdown.Trigger>
          <Dropdown.Popover placement="bottom end">
            <Dropdown.Menu
              aria-label="账户操作"
              onAction={(key) => {
                if (key === "settings") void navigate({ to: "/settings" });
                if (key === "reauth") void beginAuthentication();
                if (key === "logout") setLogoutOpen(true);
              }}
            >
              <Dropdown.Item id="identity" textValue={auth.name ?? "未设置姓名"} isDisabled>
                <strong>{auth.name ?? "未设置姓名"}</strong>
              </Dropdown.Item>
              <Dropdown.Item
                id="card"
                textValue={`校园卡号 ${profile?.campusCardNumber ?? "未设置"}`}
                isDisabled
              >
                校园卡号：{profile?.campusCardNumber ?? "未设置"}
              </Dropdown.Item>
              <Dropdown.Item id="settings">设置</Dropdown.Item>
              <Dropdown.Item id="reauth">重新认证</Dropdown.Item>
              <Dropdown.Item id="logout" variant="danger">
                注销
              </Dropdown.Item>
            </Dropdown.Menu>
          </Dropdown.Popover>
        </Dropdown>
      </header>
      <Card className="hero-card" variant="transparent" data-testid="hero-card">
        <Card.Content>
          <span className="eyebrow hero-eyebrow">WELCOME BACK</span>
          <Card.Title>{auth.name ? `${auth.name}，欢迎回来` : "欢迎回来"}</Card.Title>
          <Card.Description>从这里开始你的校园生活。</Card.Description>
        </Card.Content>
        <div className="hero-orb" aria-hidden="true">
          深
        </div>
      </Card>
      <section aria-labelledby="feature-heading">
        <h2 id="feature-heading" className="section-heading">
          功能入口
        </h2>
        <div className="feature-grid">
          {features.map(({ label, icon: FeatureIcon }) => (
            <Button
              key={label}
              className="feature-button"
              variant="outline"
              onPress={() => setPlaceholder(label)}
            >
              <span className="feature-icon" aria-hidden="true">
                <FeatureIcon size={19} focusable="false" />
              </span>
              <span className="feature-copy">
                <strong>{label}</strong>
                <small>暂未实现 · 即将支持</small>
              </span>
              <span className="feature-arrow" aria-hidden="true">
                →
              </span>
            </Button>
          ))}
        </div>
      </section>
      {placeholder && (
        <Alert className="placeholder-alert" status="default" role="status">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>{placeholder}功能即将支持</Alert.Title>
            <Alert.Description>当前仅提供入口占位。</Alert.Description>
          </Alert.Content>
          <Button
            variant="tertiary"
            onPress={() => setPlaceholder(undefined)}
            aria-label="关闭提示"
          >
            ×
          </Button>
        </Alert>
      )}
      <LogoutDialog
        isOpen={logoutOpen}
        isBusy={logoutBusy}
        onOpenChange={setLogoutOpen}
        onConfirm={async () => {
          setLogoutBusy(true);
          try {
            await invoke("logout");
            setLogoutOpen(false);
            setProfile(undefined);
            setState({ Unauthenticated: null });
          } finally {
            setLogoutBusy(false);
          }
        }}
      />
    </PageShell>
  );
}

export { authFailureFor, normalizeAuthState } from "./auth";
