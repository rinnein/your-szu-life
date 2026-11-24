import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useMemo, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  LuBell,
  LuBug,
  LuCpu,
  LuGlobe,
  LuHardDrive,
  LuInfo,
  LuKeyboard,
  LuPalette,
  LuPower,
  LuRefreshCw,
  LuServer,
  LuShieldCheck,
  LuSparkles,
  LuUser,
} from "react-icons/lu";
import { ProfileForm } from "./ProfileForm";
import { PageShell } from "./components";
import { TopAppBar } from "./shell/TopAppBar";
import { useDebugMode } from "./shell/use-debug-mode";
import { useAndroid } from "./use-android";
import { useAuth } from "./auth";
import { AdaptiveTabs, toast } from "./ui";
import { errorMessage } from "./async-state";
import AiSettingsPage from "./ai/AiSettingsPage";
import SettingsSync from "./SettingsSync";
import { LocalModelsSettings } from "./LocalModelSettings";
import McpSettings from "./McpSettings";
import { AppearanceSettings } from "./AppearanceSettings";
import { BackgroundSettings } from "./BackgroundSettings";
import { NotificationSettings } from "./NotificationSettings";
import { PermissionSettings } from "./PermissionSettings";
import { NetworkSettings } from "./NetworkSettings";
import StorageSettings from "./StorageSettings";
import { ShortcutsTable } from "./command-palette";
import AboutPage from "./AboutPage";
import DebugLogPage from "./DebugLogPage";
import { useSearch } from "@tanstack/react-router";
import type { SettingsTab } from "./routes/settings";

/**
 * tab 的顺序与展示信息。新增 tab 时必须同时改 `SETTINGS_TAB_IDS`，两处不一致
 * 会在 `renderTab` 的穷尽检查里报错。
 */
const SETTINGS_TABS = [
  { id: "profile", label: "个人信息", icon: LuUser },
  { id: "appearance", label: "外观", icon: LuPalette },
  { id: "notifications", label: "通知", icon: LuBell },
  { id: "background", label: "后台", icon: LuPower, desktopOnly: true },
  { id: "permissions", label: "权限", icon: LuShieldCheck, androidOnly: true },
  { id: "network", label: "网络", icon: LuGlobe },
  { id: "storage", label: "储存", icon: LuHardDrive },
  { id: "models", label: "本地模型", icon: LuCpu },
  { id: "ai", label: "AI 配置", icon: LuSparkles },
  { id: "mcp", label: "MCP", icon: LuServer },
  { id: "sync", label: "同步", icon: LuRefreshCw },
  { id: "shortcuts", label: "快捷键", icon: LuKeyboard },
  { id: "about", label: "关于", icon: LuInfo },
  { id: "debug", label: "调试", icon: LuBug, debugOnly: true },
] as const satisfies ReadonlyArray<{
  id: SettingsTab;
  label: string;
  icon: typeof LuUser;
  debugOnly?: boolean;
  androidOnly?: boolean;
  desktopOnly?: boolean;
}>;

export default function SettingsPage() {
  const { state, profile, setProfile } = useAuth();
  const navigation = useLogicalNavigation();
  const { enabled: debugEnabled } = useDebugMode();
  const android = useAndroid();
  const { tab: queryTab, focus } = useSearch({ from: "/settings" });

  const tabs = SETTINGS_TABS.filter(
    (item) =>
      (!("debugOnly" in item) || debugEnabled) &&
      (!("androidOnly" in item) || android === "android") &&
      (!("desktopOnly" in item) || android === "desktop"),
  );
  const requestedTab: SettingsTab = queryTab ?? (focus ? "models" : "profile");
  const activeTab: SettingsTab = tabs.some((item) => item.id === requestedTab)
    ? requestedTab
    : "profile";
  const currentTabMeta = tabs.find((t) => t.id === activeTab) ?? tabs[0];

  const authenticated = "Authenticated" in state;
  // 关于 tab 是认证页页脚的入口，未认证时也要能打开。
  const aboutOnly = !authenticated && activeTab === "about";

  useEffect(() => {
    if (authenticated || aboutOnly || "Checking" in state) return;
    void navigation.back();
  }, [authenticated, aboutOnly, state, navigation]);

  useEffect(() => {
    if (activeTab === "models" && focus) {
      const targetId = focus === "ocr" ? "local-model-ocr" : "local-model-embedding";
      const timer = setTimeout(() => {
        document.getElementById(targetId)?.scrollIntoView({ behavior: "smooth" });
      }, 50);
      return () => clearTimeout(timer);
    }
  }, [activeTab, focus]);

  const handleTabChange = (key: string) => {
    // 同一逻辑页面内的 tab 切换用 replace，不制造新的返回层级。
    void navigation.replace(`/settings?tab=${key}`);
  };

  const adaptiveItems = useMemo(
    () =>
      tabs.map((t) => ({
        value: t.id,
        label: t.label,
      })),
    [tabs],
  );

  /**
   * tab 内容的唯一分发点。用 `never` 兜底：新增 tab 却忘了给内容时编译期就报错，
   * 不会出现「切过去是空白页」。
   */
  const renderTab = (tab: SettingsTab): ReactNode => {
    switch (tab) {
      case "profile":
        return (
          <ProfileForm
            initial={profile}
            name={profile?.name}
            onSaved={async () => {
              // 保存后留在当前页：用户可能接着调其它设置，跳回首页像被踢出去。
              try {
                setProfile((await invoke("get_account_profile")) as typeof profile);
                toast.success("账户资料已保存");
              } catch (cause) {
                toast.danger("资料已保存，但未能刷新显示", { description: errorMessage(cause) });
              }
            }}
          />
        );
      case "appearance":
        return <AppearanceSettings />;
      case "notifications":
        return <NotificationSettings />;
      case "background":
        return <BackgroundSettings />;
      case "permissions":
        return <PermissionSettings />;
      case "network":
        return <NetworkSettings />;
      case "storage":
        return <StorageSettings />;
      case "models":
        return <LocalModelsSettings autoReturnKind={focus} />;
      case "ai":
        return <AiSettingsPage />;
      case "mcp":
        return <McpSettings />;
      case "sync":
        return <SettingsSync />;
      case "shortcuts":
        return <ShortcutsTable />;
      case "about":
        return <AboutPage />;
      case "debug":
        return <DebugLogPage />;
      default: {
        // 穷尽性守卫：漏掉某个 tab 时这里的赋值会失败。
        const unhandled: never = tab;
        return unhandled;
      }
    }
  };

  if (!authenticated) {
    if (!aboutOnly) return null;
    return (
      <PageShell maxWidth="lg">
        <TopAppBar title="关于与更新" description="应用信息、更新频道与检查更新" showBack={false} />
        <div className="settings-page">
          <div className="settings-panel">
            <AboutPage />
          </div>
        </div>
      </PageShell>
    );
  }

  return (
    <PageShell maxWidth="lg">
      <TopAppBar
        title="设置"
        drawerLabel="打开设置目录"
        description="管理账户资料与应用配置"
        showBack={false}
      />
      <div className="settings-page">
        <AdaptiveTabs
          label="设置分类"
          items={adaptiveItems}
          value={activeTab}
          onChange={handleTabChange}
          ariaLabel="设置导航"
          className="mb-4"
        />

        <div className="settings-panel" role="tabpanel" aria-label={currentTabMeta.label}>
          {renderTab(activeTab)}
        </div>
      </div>
    </PageShell>
  );
}
