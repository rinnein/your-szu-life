import { invoke } from "@tauri-apps/api/core";
import { trackEvent as aptabaseTrackEvent } from "@aptabase/tauri";

/**
 * 匿名统计的唯一前端出口。
 *
 * 业务代码只调用这里的 `trackAnalytics`，不要直接使用 `@aptabase/tauri`，
 * 否则会绕过用户开关。统计失败永远静默，不能影响业务流程。
 */

type Props = Record<string, string | number>;

type AnalyticsSettings = {
  enabled: boolean;
};

function nativeRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let ready: Promise<void> | null = null;
let enabled = false;

export function initializeAnalytics(): Promise<void> {
  ready ??= (async () => {
    if (!nativeRuntime()) return;
    try {
      const settings = await invoke<AnalyticsSettings>("get_analytics_settings");
      enabled = settings?.enabled !== false;
    } catch {
      // 读取失败时按关闭处理，避免在用户偏好未知时上报。
      enabled = false;
    }
  })();
  return ready;
}

export function setAnalyticsEnabled(value: boolean) {
  enabled = value;
}

export function trackAnalytics(name: string, props?: Props): void {
  if (!nativeRuntime()) return;
  void initializeAnalytics().then(async () => {
    if (!enabled) return;
    try {
      await aptabaseTrackEvent(name, props);
    } catch {
      // 网络或插件不可用不影响页面。
    }
  });
}
