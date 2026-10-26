import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  LuArrowUpDown,
  LuCpu,
  LuKeyboard,
  LuPalette,
  LuServer,
  LuSparkles,
  LuUser,
} from "react-icons/lu";
import { ProfileForm } from "./ProfileForm";
import { PageShell } from "./components";
import { TopAppBar } from "./shell/TopAppBar";
import { useAuth } from "./auth";
import AiConnectionSettings from "./AiConnectionSettings";
import SettingsTransfer from "./SettingsTransfer";
import { LocalModelsSettings } from "./LocalModelSettings";
import McpSettings from "./McpSettings";
import { AppearanceSettings } from "./AppearanceSettings";
import { ShortcutsTable } from "./command-palette";
import { useSearch } from "@tanstack/react-router";
import type { SettingsTab } from "./routes/settings";

const SETTINGS_TABS = [
  { id: "profile" as const, label: "个人信息", icon: LuUser },
  { id: "appearance" as const, label: "外观", icon: LuPalette },
  { id: "models" as const, label: "本地模型", icon: LuCpu },
  { id: "ai" as const, label: "AI配置", icon: LuSparkles },
  { id: "mcp" as const, label: "MCP", icon: LuServer },
  { id: "transfer" as const, label: "设置迁移", icon: LuArrowUpDown },
  { id: "shortcuts" as const, label: "快捷键", icon: LuKeyboard },
];

export default function SettingsPage() {
  const { state, profile, setProfile } = useAuth();
  const navigation = useLogicalNavigation();
  const { tab: queryTab, focus } = useSearch({ from: "/settings" });
  const [mobileOpen, setMobileOpen] = useState(false);

  const activeTab: SettingsTab =
    queryTab ?? (focus === "ocr" || focus === "embedding" ? "models" : "profile");

  const currentTabMeta = SETTINGS_TABS.find((t) => t.id === activeTab) ?? SETTINGS_TABS[0];
  const CurrentTabIcon = currentTabMeta.icon;

  useEffect(() => {
    if (!("Checking" in state) && !("Authenticated" in state)) void navigation.back();
  }, [navigation, state]);

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

  if (!("Authenticated" in state)) return null;

  return (
    <PageShell maxWidth="lg">
      <div className="settings-page">
        <TopAppBar title="设置" description="管理账户资料与应用配置" />

        {/* 宽屏用横向 tab，窄屏用下拉。M3 的 tab 在窄屏下会挤成两行。 */}
        <div className="settings-tabs-desktop" role="tablist" aria-label="设置导航">
          {SETTINGS_TABS.map((t) => {
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
              {SETTINGS_TABS.map((t) => {
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
          {activeTab === "models" && <LocalModelsSettings autoReturnKind={focus} />}
          {activeTab === "ai" && <AiConnectionSettings />}
          {activeTab === "mcp" && <McpSettings />}
          {activeTab === "transfer" && <SettingsTransfer />}
          {activeTab === "shortcuts" && <ShortcutsTable />}
        </div>
      </div>
    </PageShell>
  );
}
