import { useEffect } from "react";
import { useRouter } from "@tanstack/react-router";
import { installAndroidBack } from "./android-back";
import { useLogicalNavigation } from "./logical-navigation";

export function AndroidBackNavigation() {
  const router = useRouter();
  const navigation = useLogicalNavigation();
  useEffect(
    () =>
      installAndroidBack({
        canBack: () => router.history.canGoBack(),
        goBack: async () => {
          navigation.recordAction?.("back");
          router.history.back();
        },
        subscribe: (update) => router.subscribe("onResolved", update),
      }),
    [router, navigation],
  );
  return null;
}
