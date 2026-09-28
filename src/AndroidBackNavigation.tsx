import { useEffect } from "react";
import { useRouter } from "@tanstack/react-router";
import { installAndroidBack } from "./android-back";

export function AndroidBackNavigation() {
  const router = useRouter();
  useEffect(
    () =>
      installAndroidBack({
        canBack: () => router.history.canGoBack(),
        goBack: async () => {
          router.history.back();
        },
        subscribe: (update) => router.subscribe("onResolved", update),
      }),
    [router],
  );
  return null;
}
