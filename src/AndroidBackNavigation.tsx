import { useEffect } from "react";
import { useRouter } from "@tanstack/react-router";
import { installAndroidBack } from "./android-back";
import { useLogicalNavigation } from "./logical-navigation";
import { currentPageBackHandler, subscribePageBackHandler } from "./page-back";

export function AndroidBackNavigation() {
  const router = useRouter();
  const navigation = useLogicalNavigation();
  useEffect(
    () =>
      installAndroidBack({
        // 页内子状态（如 OJ 答题页）也占一次返回：有处理器时先发布 route，
        // 让原生把这次手势交回页面，而不是直接关闭子 Activity。
        canBack: () => currentPageBackHandler() !== undefined || router.history.canGoBack(),
        goBack: async () => {
          const pageBack = currentPageBackHandler();
          if (pageBack) {
            await pageBack();
            return;
          }
          navigation.recordAction?.("back");
          router.history.back();
        },
        subscribe: (update) => {
          const offRoute = router.subscribe("onResolved", update);
          const offPage = subscribePageBackHandler(update);
          return () => {
            offRoute();
            offPage();
          };
        },
      }),
    [router, navigation],
  );
  return null;
}
