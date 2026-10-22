import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Accordion, Button, Tabs } from "@heroui/react";
import { LuArrowUpDown, LuCpu, LuKeyboard, LuServer, LuSparkles, LuUser } from "react-icons/lu";
import { ProfileForm } from "./ProfileForm";
import { ModuleHeader, PageShell } from "./components";
import { useAuth } from "./auth";
import AiConnectionSettings from "./AiConnectionSettings";
import SettingsTransfer from "./SettingsTransfer";
import { LocalModelsSettings } from "./LocalModelSettings";
import McpSettings from "./McpSettings";
import { ShortcutsTable } from "./command-palette";
import { useSearch } from "@tanstack/react-router";
import type { SettingsTab } from "./routes/settings";

const SETTINGS_TABS = [
  { id: "profile" as const, label: "个人信息", icon: LuUser },
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
  const [mobileMenuKeys, setMobileMenuKeys] = useState<Set<string | number>>(() => new Set());

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
    void navigation.replace(`/settings?tab=${key}`);
  };

  if (!("Authenticated" in state)) return null;

  return (
    <PageShell maxWidth="lg">
      <div className="grid gap-6 w-full min-w-0">
        <ModuleHeader title="设置" description="管理账户资料与应用配置" />

        <Tabs
          selectedKey={activeTab}
          onSelectionChange={(key) => handleTabChange(String(key))}
          className="w-full min-w-0"
        >
          {/* 桌面端横向 Tab 导航 */}
          <div className="hidden sm:block">
            <Tabs.ListContainer>
              <Tabs.List aria-label="设置导航">
                {SETTINGS_TABS.map((t) => {
                  const Icon = t.icon;
                  return (
                    <Tabs.Tab key={t.id} id={t.id} className="flex items-center gap-2">
                      <Icon aria-hidden="true" className="size-4" />
                      {t.label}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  );
                })}
              </Tabs.List>
            </Tabs.ListContainer>
          </div>

          {/* 移动端横向空间不足时：Accordion 竖向下拉折叠切换 */}
          <div className="sm:hidden mb-1">
            <Accordion
              expandedKeys={mobileMenuKeys}
              onExpandedChange={setMobileMenuKeys}
              className="w-full"
            >
              <Accordion.Item
                id="tab-selector"
                className="rounded-xl border border-separator/60 bg-surface"
              >
                <Accordion.Heading>
                  <Accordion.Trigger className="w-full py-2.5 px-3.5 text-left text-sm font-medium flex items-center justify-between">
                    <div className="flex items-center gap-2 min-w-0">
                      <CurrentTabIcon className="size-4 text-primary shrink-0" aria-hidden="true" />
                      <span className="truncate">当前设置：{currentTabMeta.label}</span>
                    </div>
                    <Accordion.Indicator />
                  </Accordion.Trigger>
                </Accordion.Heading>
                <Accordion.Panel>
                  <Accordion.Body className="grid gap-1 p-2 pt-1 border-t border-separator/40">
                    {SETTINGS_TABS.map((t) => {
                      const isSelected = activeTab === t.id;
                      const Icon = t.icon;
                      return (
                        <Button
                          key={t.id}
                          variant={isSelected ? "primary" : "ghost"}
                          className={`w-full justify-start gap-2.5 h-9 px-3 text-xs ${
                            isSelected ? "font-semibold" : "text-muted hover:text-foreground"
                          }`}
                          onPress={() => {
                            handleTabChange(t.id);
                            setMobileMenuKeys(new Set());
                          }}
                        >
                          <Icon className="size-4 shrink-0" aria-hidden="true" />
                          <span>{t.label}</span>
                          {isSelected && <span className="ml-auto text-xs font-medium">当前</span>}
                        </Button>
                      );
                    })}
                  </Accordion.Body>
                </Accordion.Panel>
              </Accordion.Item>
            </Accordion>
          </div>

          <Tabs.Panel id="profile" className="grid gap-6 pt-4 w-full min-w-0 max-w-full">
            <ProfileForm
              initial={profile}
              name={profile?.name}
              onSaved={async () => {
                setProfile((await invoke("get_account_profile")) as typeof profile);
                await navigation.back();
              }}
            />
          </Tabs.Panel>

          <Tabs.Panel id="models" className="grid gap-6 pt-4 w-full min-w-0 max-w-full">
            <LocalModelsSettings autoReturnKind={focus} />
          </Tabs.Panel>

          <Tabs.Panel id="ai" className="grid gap-6 pt-4 w-full min-w-0 max-w-full">
            <AiConnectionSettings />
          </Tabs.Panel>

          <Tabs.Panel id="mcp" className="grid gap-6 pt-4 w-full min-w-0 max-w-full">
            <McpSettings />
          </Tabs.Panel>

          <Tabs.Panel id="transfer" className="grid gap-6 pt-4 w-full min-w-0 max-w-full">
            <SettingsTransfer />
          </Tabs.Panel>

          <Tabs.Panel id="shortcuts" className="grid gap-6 pt-4 w-full min-w-0 max-w-full">
            <ShortcutsTable />
          </Tabs.Panel>
        </Tabs>
      </div>
    </PageShell>
  );
}
