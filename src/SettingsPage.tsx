import { useLogicalNavigation } from "./logical-navigation";
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@heroui/react";
import { ProfileForm } from "./ProfileForm";
import { PageShell } from "./components";
import { useAuth } from "./auth";

export default function SettingsPage() {
  const { state, profile, setProfile } = useAuth();
  const navigation = useLogicalNavigation();

  useEffect(() => {
    if (!("Checking" in state) && !("Authenticated" in state)) void navigation.back();
  }, [navigation, state]);

  if (!("Authenticated" in state)) return null;

  return (
    <PageShell centered>
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
