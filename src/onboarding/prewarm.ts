import { invoke } from "@tauri-apps/api/core";

/**
 * 初始化向导在统一认证完成后调用：静默预热只依赖 authserver SSO 的模块（畅课、巡课）。
 *
 * 直接复用页面入口的快照命令——无本地缓存时后端会建立会话并首拉（也就是用户
 * 之后打开页面要等的那段登录），有缓存时立即返回，所以重复调用是幂等的。
 * 结果一律丢弃：失败时两个页面仍各自展示 needsAuth / 不可用提示，向导不打断用户。
 */
export function prewarmSsoModules(): void {
  for (const command of ["get_lms_snapshot", "get_jxxk_snapshot"] as const) {
    void invoke(command).catch(() => undefined);
  }
}
