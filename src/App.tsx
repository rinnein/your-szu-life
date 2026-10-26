import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type NavigateOptions } from "@tanstack/react-router";
import { Card, Dialog, LoadingIndicator, Menu, MenuItem, Button } from "material/react";
import { Banner, Kbd } from "./ui";
import { LuSearch } from "react-icons/lu";
import { openCommandPalette } from "./command-palette";
import { authFailureFor, useAuth } from "./auth";
import { ProfileForm } from "./ProfileForm";
import { BrandMark, Section } from "./components";
import { PageSurface } from "./shell/AppShell";
import { AnnouncementBoard } from "./app-content/AnnouncementBoard";
import {
  FaBug,
  FaCalendarDays,
  FaChartLine,
  FaClipboardList,
  FaCircleInfo,
  FaGear,
  FaGraduationCap,
  FaWallet,
  FaWifi,
  FaCirclePlay,
} from "react-icons/fa6";
import type { IconType } from "react-icons";

type Feature = {
  label: string;
  icon: IconType;
  description: string;
  to?: NavigateOptions["to"];
  debugOnly?: boolean;
};

const featureGroups: ReadonlyArray<{
  id: string;
  label: string;
  features: ReadonlyArray<Feature>;
}> = [
  {
    id: "study",
    label: "学习",
    features: [
      {
        label: "上课时间表",
        icon: FaCalendarDays,
        description: "查看各节次上课时间",
        to: "/class-timetable",
      },
      {
        label: "计软合集",
        icon: FaGraduationCap,
        description: "OJ等计软相关功能",
        to: "/computing",
      },
      { label: "UOOC刷课", icon: FaCirclePlay, description: "优课在线视频任务", to: "/uooc" },
      {
        label: "选课",
        icon: FaClipboardList,
        description: "查课、抢课与选课期间课表",
        to: "/enrollment",
      },
      {
        label: "学业完成查询",
        icon: FaGraduationCap,
        description: "培养方案、学分与课程完成情况",
        to: "/academic",
      },
      { label: "成绩查询", icon: FaChartLine, description: "暂未实现 · 即将支持" },
    ],
  },
  {
    id: "life",
    label: "生活",
    features: [
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
    ],
  },
  {
    id: "other",
    label: "其它",
    features: [
      { label: "WebVPN", icon: FaWifi, description: "校外访问与专属2FA认证", to: "/webvpn" },
      { label: "设置", icon: FaGear, description: "管理账户资料与应用设置", to: "/settings" },
      { label: "关于", icon: FaCircleInfo, description: "应用信息与检查更新", to: "/about" },
      {
        label: "诊断日志",
        icon: FaBug,
        description: "查看网络诊断记录",
        to: "/debug",
        debugOnly: true,
      },
    ],
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
    <Dialog
      open={isOpen}
      onClose={() => onOpenChange(false)}
      headline="确认注销？"
      subtitle="这会清除本机认证数据，需要重新完成深圳大学统一认证。"
      size="small"
      actions={
        <>
          <Button variant="text" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button
            className="button-danger"
            variant="filled"
            disabled={isBusy}
            onClick={() => void onConfirm()}
          >
            {isBusy && <LoadingIndicator size={16} />}
            {isBusy ? "注销中…" : "确认注销"}
          </Button>
        </>
      }
    >
      <p>注销后需要重新登录才能使用学习与生活相关功能。</p>
    </Dialog>
  );
}

export default function App() {
  const { state, profile, setProfile, setState, userData } = useAuth();
  const navigation = useLogicalNavigation();
  const [placeholder, setPlaceholder] = useState<string>();
  const [authBusy, setAuthBusy] = useState(false);
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
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
          detail="请在当前窗口完成深圳大学官方登录，登录成功后会自动返回工作台。"
        >
          <Button
            className="w-full"
            variant="filled"
            onClick={beginAuthentication}
            disabled={authBusy}
          >
            {authBusy && <LoadingIndicator size={16} />}
            {authBusy ? "正在打开…" : "打开认证窗口"}
          </Button>
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
          <Button
            className="w-full"
            variant="filled"
            onClick={beginAuthentication}
            disabled={authBusy}
          >
            {authBusy && <LoadingIndicator size={16} />}
            {authBusy ? "正在打开…" : failure.action}
          </Button>
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
            const saved = await invoke<any>("get_account_profile");
            setProfile(saved ?? undefined);
            if (saved)
              setState({ Authenticated: { name: saved.name, studentId: saved.studentId } });
          }}
        />
      </PageSurface>
    );
  }

  const auth =
    "Authenticated" in state ? state.Authenticated : { studentId: profile?.studentId ?? "" };
  return (
    <PageSurface maxWidth="2xl">
      <div className="grid gap-12">
        <header className="flex items-start justify-between gap-5 flex-wrap">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2.5 mb-2.5">
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
            <h1 data-webview-navigation className="text-3xl md:text-4xl font-bold tracking-tight">
              工作台
            </h1>
          </div>
          <div className="account-menu">
            <Button
              variant="text"
              className="account-trigger"
              aria-label="打开账户菜单"
              aria-haspopup="menu"
              aria-expanded={accountMenuOpen}
              onClick={() => setAccountMenuOpen((value) => !value)}
            >
              {auth.studentId}
            </Button>
            <Menu
              aria-label="账户操作"
              open={accountMenuOpen}
              onClose={() => setAccountMenuOpen(false)}
              onSelect={(event) => {
                const key = event.detail.value;
                setAccountMenuOpen(false);
                if (key === "settings") void navigation.push("/settings");
                if (key === "reauth") void beginAuthentication();
                if (key === "logout") setLogoutOpen(true);
              }}
            >
              <MenuItem value="identity" disabled>
                {auth.name ?? "未设置姓名"}
              </MenuItem>
              <MenuItem value="card" disabled>
                校园卡号：{profile?.campusCardNumber ?? "未设置"}
              </MenuItem>
              <MenuItem value="settings">设置</MenuItem>
              <MenuItem value="reauth">重新认证</MenuItem>
              <MenuItem value="logout">注销</MenuItem>
            </Menu>
          </div>
        </header>

        <div className="w-full flex justify-center">
          <button
            type="button"
            className="w-full h-11 px-4 rounded-xl border border-separator/60 bg-surface-secondary/50 hover:bg-surface-secondary transition-colors flex items-center justify-between text-muted hover:text-foreground cursor-pointer shadow-xs focus-visible:outline-2 focus-visible:outline-primary"
            onClick={() => openCommandPalette()}
            aria-label="打开命令面板快捷搜索"
          >
            <span className="flex items-center gap-2.5 text-sm">
              <LuSearch className="size-4 text-primary" aria-hidden="true" />
              <span>搜索功能或输入快捷指令</span>
            </span>
            <span className="hidden sm:inline-flex items-center gap-1">
              <Kbd>⌘ ⇧ P</Kbd>
            </span>
          </button>
        </div>

        <AnnouncementBoard />

        <div className="space-y-12">
          {featureGroups.map(({ id, label: groupLabel, features }) => (
            <Section key={id} title={groupLabel}>
              <div className="feature-grid">
                {features
                  .filter(({ debugOnly }) => !debugOnly || debugEnabled)
                  .map(({ label, icon: FeatureIcon, description, to }) => (
                    // 富内容入口（图标 + 标题 + 描述）不适合 material 的纯文本按钮，
                    // 用原生 button 承载 M3 卡片样式，文案天然可被读屏读取。
                    <button
                      key={label}
                      type="button"
                      className="feature-button"
                      onClick={() => (to ? void navigation.push(to) : setPlaceholder(label))}
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
                    </button>
                  ))}
              </div>
            </Section>
          ))}
        </div>

        {placeholder && (
          <Banner status="default" className="placeholder-alert">
            <Banner.Title>{placeholder}功能即将支持</Banner.Title>
            <Banner.Description>当前仅提供入口占位。</Banner.Description>
            <Button variant="text" onClick={() => setPlaceholder(undefined)} aria-label="关闭提示">
              ×
            </Button>
          </Banner>
        )}
      </div>

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
    </PageSurface>
  );
}

export { authFailureFor, normalizeAuthState } from "./auth";
