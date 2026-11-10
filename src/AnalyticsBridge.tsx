import { useEffect, useRef } from "react";
import { useLocation } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { initializeAnalytics, setAnalyticsEnabled, trackAnalytics } from "./analytics";

type AnalyticsSettings = {
  enabled: boolean;
};

/**
 * 匿名统计桥。
 *
 * 页面浏览只记录 pathname：不带 query/hash，因此设置页 tab 切换不会制造
 * 新的页面事件，也不会把登录跳转参数或课程参数带出去。Android 多窗口打开
 * 的新逻辑页由子 WebView 自己上报，每个 WebView 各自按 pathname 去重。
 */
export function AnalyticsBridge() {
  const location = useLocation();
  const lastPath = useRef<string | null>(null);

  useEffect(() => {
    void initializeAnalytics();
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;

    const subscription = listen<AnalyticsSettings>("analytics-settings-changed", (event) => {
      setAnalyticsEnabled(event.payload?.enabled !== false);
    });
    return () => {
      void Promise.resolve(subscription)
        .then((unlisten) => unlisten?.())
        .catch(() => undefined);
    };
  }, []);

  useEffect(() => {
    const route = location.pathname;
    if (lastPath.current === route) return;
    lastPath.current = route;
    trackAnalytics("screen_view", { route });
  }, [location.pathname]);

  return null;
}
