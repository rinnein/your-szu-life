import { useLogicalNavigation } from "./logical-navigation";
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Tabs } from "@heroui/react";
import { LuArrowUpDown, LuCpu, LuSparkles, LuUser } from "react-icons/lu";
import { ProfileForm } from "./ProfileForm";
import { ModuleHeader, PageShell } from "./components";
import { useAuth } from "./auth";
import AiConnectionSettings from "./AiConnectionSettings";
import SettingsTransfer from "./SettingsTransfer";
import { LocalModelsSettings } from "./LocalModelSettings";
import { useSearch } from "@tanstack/react-router";
import type { SettingsTab } from "./routes/settings";

export default function SettingsPage() {
  const { state, profile, setProfile } = useAuth();
  const navigation = useLogicalNavigation();
  const { tab: queryTab, focus } = useSearch({ from: "/settings" });

  const activeTab: SettingsTab =
    queryTab ?? (focus === "ocr" || focus === "embedding" ? "models" : "profile");

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
      <div className="grid gap-6">
        <ModuleHeader title="设置" description="管理账户资料与应用配置" />

        <Tabs selectedKey={activeTab} onSelectionChange={(key) => handleTabChange(String(key))}>
          <Tabs.ListContainer>
            <Tabs.List aria-label="设置导航">
              <Tabs.Tab id="profile" className="flex items-center gap-2">
                <LuUser aria-hidden="true" className="size-4" />
                个人信息
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="models" className="flex items-center gap-2">
                <LuCpu aria-hidden="true" className="size-4" />
                本地模型
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="ai" className="flex items-center gap-2">
                <LuSparkles aria-hidden="true" className="size-4" />
                AI配置
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="transfer" className="flex items-center gap-2">
                <LuArrowUpDown aria-hidden="true" className="size-4" />
                设置迁移
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>

          <Tabs.Panel id="profile" className="grid gap-6 pt-4">
            <ProfileForm
              initial={profile}
              name={profile?.name}
              onSaved={async () => {
                setProfile((await invoke("get_account_profile")) as typeof profile);
                await navigation.back();
              }}
            />
          </Tabs.Panel>

          <Tabs.Panel id="models" className="grid gap-6 pt-4">
            <LocalModelsSettings autoReturnKind={focus} />
          </Tabs.Panel>

          <Tabs.Panel id="ai" className="grid gap-6 pt-4">
            <AiConnectionSettings />
          </Tabs.Panel>

          <Tabs.Panel id="transfer" className="grid gap-6 pt-4">
            <SettingsTransfer />
          </Tabs.Panel>
        </Tabs>
      </div>
    </PageShell>
  );
}
