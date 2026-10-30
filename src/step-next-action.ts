import { useEffect, useRef } from "react";

/** 步骤交给初始化向导底栏的主操作。 */
export type StepNextAction = {
  label: string;
  disabled?: boolean;
  busy?: boolean;
  run: () => void | Promise<void>;
};

type RegisterNextAction = (action: StepNextAction | null) => void;

/**
 * 把当前步骤的主操作注册到向导底栏。
 *
 * `run` 通过 ref 转发最新实现，避免向导重渲染后步骤闭包过期；effect 只比较
 * 文案与状态，因此每次渲染传入新对象也不会触发循环更新。
 *
 * `enabled` 为 false 时不注册，用于父组件把主操作让给子组件（例如
 * AuthStep 在 AwaitingProfile 时让给 ProfileForm）。
 */
export function useRegisterNextAction(
  register: RegisterNextAction | undefined,
  action: StepNextAction | null,
  enabled = true,
) {
  const runRef = useRef(action?.run);
  runRef.current = action?.run;

  const owned = useRef(false);
  const present = action !== null;
  const label = action?.label;
  const disabled = action?.disabled;
  const busy = action?.busy;

  useEffect(() => {
    if (!register || !enabled) return;
    if (!present) {
      if (owned.current) {
        owned.current = false;
        register(null);
      }
      return;
    }
    owned.current = true;
    register({
      label: label ?? "下一步",
      disabled,
      busy,
      run: () => runRef.current?.(),
    });
  }, [register, enabled, present, label, disabled, busy]);

  useEffect(() => {
    if (!register) return;
    return () => {
      if (owned.current) {
        owned.current = false;
        register(null);
      }
    };
  }, [register]);
}
