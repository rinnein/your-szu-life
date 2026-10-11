import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProfileForm } from "./ProfileForm";
import { Card, ModuleHeader, PageShell } from "./components";
import { useAuth } from "./auth";
import AiConnectionSettings from "./AiConnectionSettings";
import SettingsTransfer from "./SettingsTransfer";

export default function SettingsPage() {
  const { state, profile, setProfile } = useAuth();
  const navigation = useLogicalNavigation();
  const [storage, setStorage] = useState<{ backend: string; available: boolean }>();

  useEffect(() => {
    let active = true;
    void invoke<{ backend: string; available: boolean }>("get_credential_storage_status")
      .then((value) => {
        if (active) setStorage(value);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!("Checking" in state) && !("Authenticated" in state)) void navigation.back();
  }, [navigation, state]);

  if (!("Authenticated" in state)) return null;

  return (
    <PageShell maxWidth="lg">
      <div className="grid gap-6">
        <ModuleHeader title="设置" description="管理账户资料与应用配置" />

        {storage && (
          <Card
            title="隐私信息存储"
            description={
              !storage.available
                ? "凭据存储暂时不可用，请检查密钥环或本机文件"
                : storage.backend === "localEncryptedFile"
                  ? "当前使用本机加密文件，保护依赖当前系统账号的文件访问权限"
                  : "当前使用系统密钥环"
            }
          >
            <div className="h-0" />
          </Card>
        )}

        <AiConnectionSettings />

        <SettingsTransfer />

        <ProfileForm
          initial={profile}
          name={profile?.name}
          onSaved={async () => {
            setProfile((await invoke("get_account_profile")) as typeof profile);
            await navigation.back();
          }}
        />
      </div>
    </PageShell>
  );
}
