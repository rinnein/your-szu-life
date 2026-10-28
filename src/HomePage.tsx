import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Dialog, LoadingIndicator, Menu, MenuItem } from "material/react";
import { useAuth, beginAuthentication } from "./auth";
import { BrandMark } from "./components";
import { PageSurface } from "./shell/AppShell";
import { TopAppBar } from "./shell/TopAppBar";
import { AuthGate } from "./shell/AuthGate";
import { useDebugMode } from "./shell/use-debug-mode";
import { useLogicalNavigation } from "./logical-navigation";
import { AnnouncementBoard } from "./app-content/AnnouncementBoard";

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
        <div className="grid gap-12">
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
                  variant="text"
                  className="account-trigger"
                  aria-label="打开账户菜单"
                  aria-haspopup="menu"
                  aria-expanded={accountMenuOpen}
                  disabled={authBusy}
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
                    if (key === "reauth") void reauthenticate();
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
            }
          />

          <AnnouncementBoard />
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
