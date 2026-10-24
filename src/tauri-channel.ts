import * as tauriCore from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";

export type ChannelLike<T> = {
  onmessage: ((message: T) => void) | null;
  [key: string]: unknown;
};

function getChannelClass(): (new <U>() => ChannelLike<U>) | undefined {
  try {
    return (tauriCore as { Channel?: new <U>() => ChannelLike<U> }).Channel;
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
 */
export function useChannelSubscription<T>(
  command: string,
  onMessage: (event: T) => void,
  args: Record<string, unknown> = {},
) {
  const onMessageRef = useRef(onMessage);
  onMessageRef.current = onMessage;

  useEffect(() => {
    let active = true;
    const channel = createTaskChannel<T>((message) => {
      if (active) {
        onMessageRef.current(message);
      }
    });

    try {
      void tauriCore.invoke(command, { ...args, channel }).catch((err) => {
        console.warn(`[Channel] 订阅命令 ${command} 失败:`, err);
      });
    } catch {
      // Mock or non-tauri environment safe
    }

    return () => {
      active = false;
    };
  }, [command]);
}
