/**
 * `@tauri-apps/api/event` 的演示模式替身。
 *
 * 演示模式（纯浏览器渲染）下没有 Tauri IPC，`listen` 会 reject 并把
 * AppContentProvider 等全局桥打成错误态。这里在演示模式返回不落地的
 * 取消函数；非演示模式原样转发到包内真实实现。
 */
import { demoActive } from "./demo-state";

export * from "../../node_modules/@tauri-apps/api/event.js";

type UnlistenFn = () => void;

export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<UnlistenFn> {
  if (demoActive()) {
    return () => undefined;
  }
  return (await import("../../node_modules/@tauri-apps/api/event.js")).listen(event, handler);
}
