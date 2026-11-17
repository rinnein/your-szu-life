/**
 * 签到自动化开关的读取与保存。
 *
 * 读取失败时只把控件置灰并提供重试，**不能**把默认值写回后端；保存采用乐观
 * 更新 + 串行队列：快速连续切换两个开关时只按最终值发一次请求，失败回滚到
 * 最后一次确认落盘的值。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, useAsyncLoad } from "../async-state";
import { toast } from "../ui";
import { usePending } from "../use-pending";
import type { SigninAutomationSettings } from "./types";

const DEFAULT_SETTINGS: SigninAutomationSettings = {
  lmsAutoRadar: false,
  szuEnabled: false,
};

export type AutomationField = keyof SigninAutomationSettings;

export function useSigninAutomation() {
  const pending = usePending();
  const { state, reload } = useAsyncLoad<SigninAutomationSettings>(() =>
    invoke<SigninAutomationSettings>("signin_get_automation_settings"),
  );
  const [settings, setSettings] = useState<SigninAutomationSettings | null>(null);
  /** 最后一次确认落盘的设置，用于失败回滚。 */
  const confirmed = useRef<SigninAutomationSettings | null>(null);
  /** 队列里最新一次期望写入的设置。 */
  const latest = useRef<SigninAutomationSettings | null>(null);
  const queue = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (state.status !== "ready") return;
    confirmed.current = state.data;
    latest.current = state.data;
    setSettings(state.data);
  }, [state]);

  const flush = useCallback(async () => {
    const next = latest.current;
    if (!next) return;
    const result = await pending.run("automation", () =>
      invoke<SigninAutomationSettings>("signin_set_automation_settings", { settings: next }),
    );
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      const base = confirmed.current;
      latest.current = base;
      setSettings(base);
      toast.danger("保存自动签到设置失败", { description: errorMessage(result.error) });
      return;
    }
    const saved = result.value;
    confirmed.current = saved;
    // 队列里已有更新值时不要用旧响应覆盖它，让下一次 flush 继续。
    if (latest.current === next) {
      latest.current = saved;
      setSettings(saved);
    }
  }, [pending]);

  const toggle = useCallback(
    (key: AutomationField, checked: boolean) => {
      if (!latest.current) return;
      const next = { ...latest.current, [key]: checked };
      latest.current = next;
      setSettings(next);
      queue.current = queue.current.then(flush, flush);
    },
    [flush],
  );

  const loading = state.status === "idle" || state.status === "loading";
  const failed = state.status === "error";
  return {
    state,
    loading,
    failed,
    settings: settings ?? DEFAULT_SETTINGS,
    toggle,
    pending,
    reload,
  };
}
