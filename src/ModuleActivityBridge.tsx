import { useEffect } from "react";
import { useLocation } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";

const MODULE_PREFIXES: [string, string][] = [
  ["/study/oj", "oj"],
  ["/study/uooc", "uooc"],
  ["/study/patrol", "jxxk"],
  ["/study/enrollment", "enrollment"],
  ["/study/academic", "enrollment"],
  ["/study/lms", "lms"],
  ["/life/campus-network", "campus-network"],
  ["/life/campus-payments", "electricity"],
];

export function moduleForPath(pathname: string): string | undefined {
  const path = pathname.split(/[?#]/, 1)[0];
  return MODULE_PREFIXES.find(([prefix]) => path === prefix || path.startsWith(`${prefix}/`))?.[1];
}

/**
 * 把当前路由对应的功能模块上报给后端，供后台轮询选择 Active 档位。
 *
 * 首页不映射任何模块：它只订阅 Channel，不代表对应功能正在使用。
 */
export function ModuleActivityBridge() {
  const location = useLocation();
  const module = moduleForPath(location.pathname);
  useEffect(() => {
    if (!module) return;
    void invoke("set_active_module", { module, active: true }).catch(() => {});
    return () => {
      void invoke("set_active_module", { module, active: false }).catch(() => {});
    };
  }, [module]);
  return null;
}
