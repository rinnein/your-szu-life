import { useEffect, useRef, useState } from "react";
import { useRouterState } from "@tanstack/react-router";
import { toast } from "@heroui/react";
import { useLogicalNavigation } from "../logical-navigation";
import { useAppContent } from "./context";

export function UpdateNotice() {
  const { snapshot } = useAppContent();
  const navigation = useLogicalNavigation();
  const path = useRouterState({ select: (state) => state.location.pathname });
  const [dismissedVersion, setDismissedVersion] = useState<string>();
  const activeToastIdRef = useRef<string | undefined>(undefined);
  const latest = snapshot?.update.data?.latest_ver;

  useEffect(() => {
    const shouldShow =
      Boolean(snapshot?.updateAvailable) &&
      Boolean(latest) &&
      latest !== dismissedVersion &&
      path !== "/about";

    if (!shouldShow) {
      if (activeToastIdRef.current !== undefined) {
        toast.close(activeToastIdRef.current);
        activeToastIdRef.current = undefined;
      }
      return;
    }

    if (activeToastIdRef.current === undefined && latest) {
      const description = snapshot?.update.fromCache
        ? "来自上次成功检查；可在关于页重新检查。"
        : "查看更新日志，按需前往下载。";

      const id = toast.info(`发现新版本 ${latest}`, {
        description,
        actionProps: {
          children: "查看更新",
          onPress: () => {
            setDismissedVersion(latest);
            toast.close(id);
            activeToastIdRef.current = undefined;
            void navigation.push("/about");
          },
        },
        onClose: () => {
          setDismissedVersion(latest);
          activeToastIdRef.current = undefined;
        },
        timeout: 0,
      });
      activeToastIdRef.current = id;
    }
  }, [
    snapshot?.updateAvailable,
    snapshot?.update.fromCache,
    latest,
    dismissedVersion,
    path,
    navigation,
  ]);

  useEffect(() => {
    return () => {
      if (activeToastIdRef.current !== undefined) {
        toast.close(activeToastIdRef.current);
        activeToastIdRef.current = undefined;
      }
    };
  }, []);

  return null;
}
