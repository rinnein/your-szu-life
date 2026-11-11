import { useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "../ui";
import { registerDebugTap } from "./use-debug-mode";

/**
 * 向导欢迎页的 Logo 连击：与首页调试模式同一套 5 连击手势（2 秒窗口），
 * 但触发的是演示模式 + 调试模式。激活成功后后端会广播模拟的登录状态，
 * OnboardingGate 据此自动切到主应用。
 */
export function useDemoModeActivation() {
  const tapTimes = useRef<number[]>([]);

  return useCallback(() => {
    const result = registerDebugTap(tapTimes.current, Date.now());
    if (!result.activated) {
      tapTimes.current = result.taps;
      return;
    }
    tapTimes.current = [];
    void invoke("set_demo_mode", { enabled: true })
      .then(() => {
        toast.info("已进入演示模式", { description: "界面数据均为模拟数据" });
      })
      .catch((cause) => {
        toast.danger("进入演示模式失败", { description: String(cause) });
      });
  }, []);
}
