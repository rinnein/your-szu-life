import { createElement, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Dialog, LoadingIndicator, MenuItem } from "material/react";
import "material/elements/css/menu";
import { defineMenu } from "material/elements";
import { useAuth, beginAuthentication } from "./auth";
import { BrandMark } from "./components";
import { PageSurface } from "./shell/PageSurface";
import { TopAppBar } from "./shell/TopAppBar";
import { AuthGate } from "./shell/AuthGate";
import { useDebugMode } from "./shell/use-debug-mode";
import { useLogicalNavigation } from "./logical-navigation";
import { AnnouncementBoard } from "./app-content/AnnouncementBoard";
import { HomeCalendarWidget } from "./home-schedule/HomeCalendarWidget";
import { ElectricityMonitorProvider } from "./electricity-monitor";

// AccountMenu 直接渲染 <m-menu>，因此由页面自己注册菜单元素与样式。
defineMenu();

/**
 * 首页。
 *
 * 顶级部件之一，也是逻辑导航栈的 root：只承载账户入口与公告栏，
 * 学习 / 生活相关的入口分别在学习页与生活页。
 */
export default function HomePage() {
  const { state, profile, setProfile, setState } = useAuth();
  const navigation = useLogicalNavigation();
  const { handleTap } = useDebugMode();
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [logoutBusy, setLogoutBusy] = useState(false);
  const [authBusy, setAuthBusy] = useState(false);

  const reauthenticate = async () => {
    setAuthBusy(true);
    try {
      await beginAuthentication();
    } finally {
      setAuthBusy(false);
    }
  };

  const auth =
    "Authenticated" in state ? state.Authenticated : { studentId: profile?.studentId ?? "" };

  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar
          title="首页"
          eyebrow="YOUR SZU LIFE"
          showBack={false}
          leading={
            <button
              type="button"
              className="brand-mark-trigger"
              aria-label="应用图标，连续点击 5 次启用调试模式"
              onClick={handleTap}
            >
              <BrandMark />
            </button>
          }
          actions={
            <div className="account-menu">
              <Button
                id="account-trigger"
                variant="text"
                className="account-trigger"
                aria-label="打开账户菜单"
                aria-haspopup="menu"
                aria-expanded={accountMenuOpen}
                disabled={authBusy}
              >
                {auth.studentId}
              </Button>
              <AccountMenu
                open={accountMenuOpen}
                onOpenChange={setAccountMenuOpen}
                onSelect={(key) => {
                  if (key === "settings") void navigation.push("/settings");
                  if (key === "reauth") void reauthenticate();
                  if (key === "logout") setLogoutOpen(true);
                }}
                identity={auth.name ?? "未设置姓名"}
                campusCardNumber={profile?.campusCardNumber ?? "未设置"}
              />
            </div>
          }
        />

        <div className="grid gap-12">
          <AnnouncementBoard />
          <ElectricityMonitorProvider>
            <HomeCalendarWidget />
          </ElectricityMonitorProvider>
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
    </AuthGate>
  );
}

/**
 * 账户菜单。
 *
 * 这里直接渲染 `<m-menu>`，不用 material 的 React `Menu` 包装：包装组件把
 * `anchor` 作为属性在挂载后的 effect 里写入，而 material 3.2.4 的定位只认
 * 创建菜单时的 opener，弹层因此会退回屏幕左上角。直接渲染宿主元素并让 React
 * 在连接前写入 `anchor`，首次挂载与重新挂载都能锚定到账户按钮。
 */
function AccountMenu({
  open,
  onOpenChange,
  onSelect,
  identity,
  campusCardNumber,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (key: string) => void;
  identity: string;
  campusCardNumber: string;
}) {
  const menuRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const handleOpen = () => onOpenChange(true);
    const handleClose = () => onOpenChange(false);
    const handleSelect = (event: Event) => {
      onOpenChange(false);
      const key = (event as CustomEvent<{ value?: string }>).detail?.value;
      if (key) onSelect(key);
    };
    menu.addEventListener("open", handleOpen);
    menu.addEventListener("close", handleClose);
    menu.addEventListener("select", handleSelect);
    return () => {
      menu.removeEventListener("open", handleOpen);
      menu.removeEventListener("close", handleClose);
      menu.removeEventListener("select", handleSelect);
    };
  }, [onOpenChange, onSelect]);

  return createElement(
    "m-menu",
    {
      ref: menuRef,
      "aria-label": "账户操作",
      anchor: "account-trigger",
      position: "bottom-end",
      open: open ? "" : undefined,
    },
    createElement(MenuItem, { key: "identity", value: "identity", disabled: true }, identity),
    createElement(
      MenuItem,
      { key: "card", value: "card", disabled: true },
      `校园卡号：${campusCardNumber}`,
    ),
    createElement(MenuItem, { key: "settings", value: "settings" }, "设置"),
    createElement(MenuItem, { key: "reauth", value: "reauth" }, "重新认证"),
    createElement(MenuItem, { key: "logout", value: "logout" }, "注销"),
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
