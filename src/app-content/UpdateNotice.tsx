import { useLogicalNavigation } from "../logical-navigation";
import { useState } from "react";
import { useRouterState } from "@tanstack/react-router";
import { Alert, Button, CloseButton } from "@heroui/react";
import { useAppContent } from "./context";

export function UpdateNotice() {
  const { snapshot } = useAppContent();
  const navigation = useLogicalNavigation();
  const path = useRouterState({ select: (state) => state.location.pathname });
  const [dismissedVersion, setDismissedVersion] = useState<string>();
  const latest = snapshot?.update.data?.latest_ver;
  if (!snapshot?.updateAvailable || !latest || latest === dismissedVersion || path === "/about")
    return null;
  return (
    <aside className="app-update-notice" aria-label="应用更新提示">
      <Alert status="accent" role="status">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>发现新版本 {latest}</Alert.Title>
          <Alert.Description>
            {snapshot.update.fromCache
              ? "来自上次成功检查；可在关于页重新检查。"
              : "查看更新日志，按需前往下载。"}
          </Alert.Description>
          <Button
            variant="secondary"
            size="sm"
            onPress={() => {
              setDismissedVersion(latest);
              void navigation.push("/about");
            }}
          >
            查看更新
          </Button>
        </Alert.Content>
        <CloseButton aria-label="关闭更新提示" onPress={() => setDismissedVersion(latest)} />
      </Alert>
    </aside>
  );
}
