import { useEffect, useState } from "react";
import { isAndroidPlatform } from "./os-info";

/**
 * 平台判定（异步）收敛成三态：unknown 表示还没拿到结果。
 *
 * 直接 `useState(false)` + `useEffect` 会让 Android 首帧先渲染桌面分支的
 * 操作区再翻转；调用方在 unknown 时不渲染平台相关 UI 即可避免闪烁。
 * 测试沿用对 `../os-info` / `./os-info` 的模块 mock。
 */
export type PlatformState = "unknown" | "android" | "desktop";

export function useAndroid(): PlatformState {
  const [state, setState] = useState<PlatformState>("unknown");
  useEffect(() => {
    let active = true;
    void isAndroidPlatform().then((android) => {
      if (active) setState(android ? "android" : "desktop");
    });
    return () => {
      active = false;
    };
  }, []);
  return state;
}
