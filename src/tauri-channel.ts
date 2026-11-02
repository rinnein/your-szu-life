import * as tauriCore from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";

export type ChannelLike<T> = {
  onmessage: ((message: T) => void) | null;
  [key: string]: unknown;
};

function getChannelClass(): (new <U>() => ChannelLike<U>) | undefined {
  try {
    return (tauriCore as unknown as { Channel?: new <U>() => ChannelLike<U> }).Channel;
  } catch {
    return undefined;
  }
}

/**
 * 为一次性流式长任务创建 Channel 辅助函数。
 * 允许调用方传入 onMessage 回调，并返回一个配置好 channel 的实例。
 * 兼容未在 mock 中导出 Channel 的单元测试环境。
 */
export function createTaskChannel<T>(onMessage: (event: T) => void): ChannelLike<T> {
  let channel: ChannelLike<T>;
  const ChannelClass = getChannelClass();
  if (typeof ChannelClass === "function") {
    try {
      channel = new ChannelClass<T>();
    } catch {
      channel = { onmessage: null };
    }
  } else {
    channel = { onmessage: null };
  }
  channel.onmessage = onMessage;
  return channel;
}

/**
 * 通用 Tauri Channel 订阅 Hook。
 * 在组件生命周期内维护一个 Channel 实例向 Rust 注册订阅。
 * 当组件卸载时将自动切断连接，Rust 订阅者池检测到通道关闭后自动释放资源，避免事件泄漏。
 *
 * `options.key` 变化时会重新订阅（用于按站点/学期等参数切换的订阅）；
 * `options.onError` 用于把订阅失败暴露给调用方。
 */
export function useChannelSubscription<T>(
  command: string,
  onMessage: (event: T) => void,
  args: Record<string, unknown> = {},
  options?: { key?: string | number; onError?: (error: unknown) => void; enabled?: boolean },
) {
  const onMessageRef = useRef(onMessage);
  onMessageRef.current = onMessage;
  const onErrorRef = useRef(options?.onError);
  onErrorRef.current = options?.onError;
  const argsRef = useRef(args);
  argsRef.current = args;
  const key = options?.key ?? "";
  const enabled = options?.enabled ?? true;

  useEffect(() => {
    if (!enabled) return;
    let active = true;
    const channel = createTaskChannel<T>((message) => {
      if (active) {
        onMessageRef.current(message);
      }
    });

    try {
      void tauriCore.invoke(command, { ...argsRef.current, channel }).catch((err) => {
        console.warn(`[Channel] 订阅命令 ${command} 失败:`, err);
        onErrorRef.current?.(err);
      });
    } catch (err) {
      // Mock or non-tauri environment safe
      onErrorRef.current?.(err);
    }

    return () => {
      active = false;
    };
  }, [command, key, enabled]);
}
