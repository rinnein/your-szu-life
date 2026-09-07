import { useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@heroui/react";
import { ProfileForm } from "./ProfileForm";
import { PageShell } from "./components";
import { useAuth } from "./auth";

export default function SettingsPage() {
  const { state, profile, setProfile } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    if (!("Checking" in state) && !("Authenticated" in state)) void navigate({ to: "/" });
  }, [navigate, state]);

  if (!("Authenticated" in state)) return null;

  return (
    <PageShell centered>
      <ProfileForm
        initial={profile}
        name={profile?.name}
        onSaved={async () => {
          setProfile((await invoke("get_account_profile")) as typeof profile);
          await navigate({ to: "/" });
        }}
      />
      <Button className="back-button" variant="tertiary" onPress={() => void navigate({ to: "/" })}>
        返回工作台
      </Button>
    </PageShell>
  );
}
