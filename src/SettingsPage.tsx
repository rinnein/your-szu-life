import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  LuArrowUpDown,
  LuBell,
  LuBug,
  LuCpu,
  LuGlobe,
  LuInfo,
  LuKeyboard,
  LuPalette,
  LuServer,
  LuSparkles,
  LuUser,
} from "react-icons/lu";
import { ProfileForm } from "./ProfileForm";
import { PageShell } from "./components";
import { TopAppBar } from "./shell/TopAppBar";
import { useDebugMode } from "./shell/use-debug-mode";
import { useAuth } from "./auth";
import AiConnectionSettings from "./AiConnectionSettings";
import SettingsTransfer from "./SettingsTransfer";
import { LocalModelsSettings } from "./LocalModelSettings";
import McpSettings from "./McpSettings";
import { AppearanceSettings } from "./AppearanceSettings";
import { NotificationSettings } from "./NotificationSettings";
import { NetworkSettings } from "./NetworkSettings";
import { ShortcutsTable } from "./command-palette";
import AboutPage from "./AboutPage";
import DebugLogPage from "./DebugLogPage";
import { useSearch } from "@tanstack/react-router";
import type { SettingsTab } from "./routes/settings";

const SETTINGS_TABS: ReadonlyArray<{
  id: SettingsTab;
  label: string;
  icon: typeof LuUser;
  /** 仅在调试模式启用后出现。 */
  debugOnly?: boolean;
}> = [
  { id: "profile", label: "个人信息", icon: LuUser },
  { id: "appearance", label: "外观", icon: LuPalette },
  { id: "notifications", label: "通知", icon: LuBell },
  { id: "network", label: "网络", icon: LuGlobe },
  { id: "models", label: "本地模型", icon: LuCpu },
  { id: "ai", label: "AI配置", icon: LuSparkles },
  { id: "mcp", label: "MCP", icon: LuServer },
  { id: "transfer", label: "设置迁移", icon: LuArrowUpDown },
  { id: "shortcuts", label: "快捷键", icon: LuKeyboard },
  { id: "about", label: "关于", icon: LuInfo },
  { id: "debug", label: "诊断日志", icon: LuBug, debugOnly: true },
];

export default function SettingsPage() {
  const { state, profile, setProfile } = useAuth();
  const navigation = useLogicalNavigation();
  const { enabled: debugEnabled } = useDebugMode();
  const { tab: queryTab, focus } = useSearch({ from: "/settings" });
  const [mobileOpen, setMobileOpen] = useState(false);

  const tabs = SETTINGS_TABS.filter((item) => !item.debugOnly || debugEnabled);
  const requestedTab: SettingsTab = queryTab ?? (focus ? "models" : "profile");
  const activeTab: SettingsTab = tabs.some((item) => item.id === requestedTab)
    ? requestedTab
    : "profile";
  const currentTabMeta = tabs.find((t) => t.id === activeTab) ?? tabs[0];
  const CurrentTabIcon = currentTabMeta.icon;

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
    setMobileOpen(false);
    // 同一逻辑页面内的 tab 切换用 replace，不制造新的返回层级。
    void navigation.replace(`/settings?tab=${key}`);
  };

  if (!authenticated) {
    if (!aboutOnly) return null;
    return (
      <PageShell maxWidth="lg">
        <TopAppBar title="关于" description="应用信息与检查更新" showBack={false} />
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
      <TopAppBar title="设置" description="管理账户资料与应用配置" showBack={false} />
      <div className="settings-page">
        {/* 宽屏用横向 tab，窄屏用下拉。M3 的 tab 在窄屏下会挤成两行。 */}
        <div className="settings-tabs-desktop" role="tablist" aria-label="设置导航">
          {tabs.map((t) => {
            const Icon = t.icon;
            const selected = activeTab === t.id;
            return (
              <button
                key={t.id}
                type="button"
                role="tab"
                className="settings-tab"
                aria-selected={selected}
                data-selected={selected || undefined}
                onClick={() => handleTabChange(t.id)}
              >
                <Icon aria-hidden="true" className="settings-tab-icon" />
                <span>{t.label}</span>
              </button>
            );
          })}
        </div>

        <div className="settings-tabs-mobile">
          <button
            type="button"
            className="settings-mobile-trigger"
            aria-expanded={mobileOpen}
            aria-controls="settings-mobile-menu"
            onClick={() => setMobileOpen((value) => !value)}
          >
            <CurrentTabIcon aria-hidden="true" className="settings-tab-icon" />
            <span>当前设置：{currentTabMeta.label}</span>
            <span className="settings-mobile-chevron" aria-hidden="true">
              {mobileOpen ? "⌃" : "⌄"}
            </span>
          </button>
          {mobileOpen && (
            <div id="settings-mobile-menu" className="settings-mobile-menu" role="menu">
              {tabs.map((t) => {
                const Icon = t.icon;
                const selected = activeTab === t.id;
                return (
                  <button
                    key={t.id}
                    type="button"
                    role="menuitem"
                    className="settings-mobile-item"
                    data-selected={selected || undefined}
                    onClick={() => handleTabChange(t.id)}
                  >
                    <Icon aria-hidden="true" className="settings-tab-icon" />
                    <span>{t.label}</span>
                    {selected && <span className="settings-mobile-current">当前</span>}
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div className="settings-panel" role="tabpanel" aria-label={currentTabMeta.label}>
          {activeTab === "profile" && (
            <ProfileForm
              initial={profile}
              name={profile?.name}
              onSaved={async () => {
                setProfile((await invoke("get_account_profile")) as typeof profile);
                await navigation.back();
              }}
            />
          )}
          {activeTab === "appearance" && <AppearanceSettings />}
          {activeTab === "notifications" && <NotificationSettings />}
          {activeTab === "network" && <NetworkSettings />}
          {activeTab === "models" && <LocalModelsSettings autoReturnKind={focus} />}
          {activeTab === "ai" && <AiConnectionSettings />}
          {activeTab === "mcp" && <McpSettings />}
          {activeTab === "transfer" && <SettingsTransfer />}
          {activeTab === "shortcuts" && <ShortcutsTable />}
          {activeTab === "about" && <AboutPage />}
          {activeTab === "debug" && <DebugLogPage />}
        </div>
      </div>
    </PageShell>
  );
}
