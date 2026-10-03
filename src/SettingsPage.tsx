import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card } from "@heroui/react";
import { ProfileForm } from "./ProfileForm";
import { PageShell } from "./components";
import { useAuth } from "./auth";

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
    <PageShell centered>
      {storage && (
        <Card>
          <Card.Header>
            <Card.Title>隐私信息存储</Card.Title>
            <Card.Description>
              {!storage.available
                ? "凭据存储暂时不可用，请检查密钥环或本机文件。"
                : storage.backend === "localEncryptedFile"
                  ? "当前使用本机加密文件，保护依赖当前系统账号的文件访问权限。"
                  : "当前使用系统密钥环。"}
            </Card.Description>
          </Card.Header>
        </Card>
      )}
      <ProfileForm
        initial={profile}
        name={profile?.name}
        onSaved={async () => {
          setProfile((await invoke("get_account_profile")) as typeof profile);
          await navigation.back();
        }}
      />
      <Button
        data-webview-navigation
        className="back-button"
        variant="tertiary"
        onPress={() => void navigation.back()}
      >
        返回工作台
      </Button>
    </PageShell>
  );
}
