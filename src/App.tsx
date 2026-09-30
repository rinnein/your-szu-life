import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type NavigateOptions } from "@tanstack/react-router";
import { Alert, AlertDialog, Button, Card, Dropdown, Spinner } from "@heroui/react";
import { authFailureFor, useAuth } from "./auth";
import { ProfileForm } from "./ProfileForm";
import { BrandMark, PageShell } from "./components";
import { AnnouncementBoard } from "./app-content/AnnouncementBoard";
import {
  FaBug,
  FaCalendarDays,
  FaChartLine,
  FaClipboardList,
  FaGraduationCap,
  FaWallet,
  FaWifi,
} from "react-icons/fa6";
import type { IconType } from "react-icons";

type Feature = {
  label: string;
  icon: IconType;
  description: string;
  to?: NavigateOptions["to"];
  debugOnly?: boolean;
};

const features: ReadonlyArray<Feature> = [
  {
    label: "上课时间表",
    icon: FaCalendarDays,
    description: "查看各节次上课时间",
    to: "/class-timetable",
  },
  { label: "选课", icon: FaClipboardList, description: "暂未实现 · 即将支持" },
  { label: "培养方案检查", icon: FaGraduationCap, description: "暂未实现 · 即将支持" },
  { label: "成绩查询", icon: FaChartLine, description: "暂未实现 · 即将支持" },
  {
    label: "校园网登录",
    icon: FaWifi,
    description: "教工区 + 宿舍区 | 自动登录",
    to: "/campus-network",
  },
  {
    label: "校园卡/电费/网费",
    icon: FaWallet,
    description: "快速导航入口",
    to: "/campus-payments",
  },
  {
    label: "诊断日志",
    icon: FaBug,
    description: "查看网络诊断记录",
    to: "/debug",
    debugOnly: true,
  },
];

export function registerDebugTap(taps: number[], now: number) {
  const recent = taps.filter((timestamp) => now - timestamp <= 2000);
  recent.push(now);
  return { activated: recent.length >= 5, taps: recent.length >= 5 ? [] : recent };
}

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
  const navigation = useLogicalNavigation();
  const [placeholder, setPlaceholder] = useState<string>();
  const [authBusy, setAuthBusy] = useState(false);
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [logoutBusy, setLogoutBusy] = useState(false);
  const [debugEnabled, setDebugEnabled] = useState(false);
  const debugTapTimes = useRef<number[]>([]);

  useEffect(() => {
    if (!("Authenticated" in state)) return;
    let active = true;
    void invoke("get_debug_mode")
      .then((value) => {
        if (active) setDebugEnabled(Boolean((value as { enabled?: boolean })?.enabled));
      })
      .catch(() => undefined);
    let unlisten: (() => void) | undefined;
    void listen<{ enabled: boolean }>("debug-mode-changed", (event) => {
      if (active) setDebugEnabled(event.payload.enabled);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [state]);

  const handleBrandMarkTap = () => {
    const result = registerDebugTap(debugTapTimes.current, Date.now());
    if (result.activated) {
      debugTapTimes.current = [];
      void invoke("set_debug_mode", { enabled: true })
        .then(() => setDebugEnabled(true))
        .catch(() => undefined);
      return;
    }
    debugTapTimes.current = result.taps;
  };

  const beginAuthentication = async () => {
    setAuthBusy(true);
    try {
      await invoke("begin_authentication", { userAgent: navigator.userAgent });
    } finally {
      setAuthBusy(false);
    }
  };

  if ("Checking" in state)
    return (
      <PageShell centered showAbout>
        <Card className="status-card" variant="default">
          <Spinner size="lg" />
          <Card.Title>正在检查统一认证状态…</Card.Title>
        </Card>
      </PageShell>
    );
  if ("Unauthenticated" in state)
    return (
      <PageShell centered showAbout>
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
      <PageShell centered showAbout>
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
      <PageShell centered showAbout>
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
      <PageShell centered showAbout>
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
    <PageShell showAbout>
      <header className="app-header">
        <div>
          <div className="brand-line">
            <button
              type="button"
              className="brand-mark-trigger"
              aria-label="应用图标，连续点击 5 次启用调试模式"
              onClick={handleBrandMarkTap}
            >
              <BrandMark />
            </button>
            <span className="eyebrow">YOUR SZU LIFE</span>
          </div>
          <h1 data-webview-navigation>深大生活工作台</h1>
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
                if (key === "settings") void navigation.push("/settings");
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
      <AnnouncementBoard />
      <section aria-labelledby="feature-heading">
        <h2 id="feature-heading" className="section-heading">
          功能入口
        </h2>
        <div className="feature-grid">
          {features
            .filter(({ debugOnly }) => !debugOnly || debugEnabled)
            .map(({ label, icon: FeatureIcon, description, to }) => (
              <Button
                key={label}
                className="feature-button"
                variant="outline"
                onPress={() => (to ? void navigation.push(to) : setPlaceholder(label))}
              >
                <span className="feature-icon" aria-hidden="true">
                  <FeatureIcon size={19} focusable="false" />
                </span>
                <span className="feature-copy">
                  <strong>{label}</strong>
                  <small>{description}</small>
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
