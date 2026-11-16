export const WECHAT_SERVICES = [
  {
    id: "electricity",
    title: "电费",
    description: "前往官方微信服务办理宿舍电费。",
    url: "https://weixiao.qq.com/apps/public/service/index.html?hid=bf11210a3cc169eaa981beeb476596f6&media_id=gh_a280c83dc0be",
  },
  {
    id: "campus-card",
    title: "校园卡",
    description: "前往官方微信服务充值或查询校园卡。",
    url: "https://ecardwx.szu.edu.cn/wechat/home/index.html",
  },
] as const;

export const CAMPUS_PAYMENTS_LAST_ROUTE_KEY = "your-szu-life.campus-payments.last-route";

/**
 * 校园服务的四个入口（tab 顺序即路由的单一来源）。
 *
 * 它们是同一逻辑页面内的内容切换（各自只是不同的服务面板），因此用 tab 的
 * 语义表达，并用 `replace` 同步 URL——不制造新的返回层级。
 */
export const CAMPUS_PAYMENTS_SERVICES = [
  { path: "/life/campus-payments/card", label: "校园卡" },
  { path: "/life/campus-payments/network", label: "网费" },
  { path: "/life/campus-payments/electricity", label: "查电量" },
  { path: "/life/campus-payments/recharge", label: "充电费" },
] as const;

export const CAMPUS_PAYMENTS_ROUTES: readonly string[] = CAMPUS_PAYMENTS_SERVICES.map(
  (service) => service.path,
);

export function readLastCampusPaymentsRoute() {
  try {
    return globalThis.localStorage?.getItem(CAMPUS_PAYMENTS_LAST_ROUTE_KEY) ?? null;
  } catch {
    return null;
  }
}

export function saveLastCampusPaymentsRoute(pathname: string) {
  try {
    globalThis.localStorage?.setItem(CAMPUS_PAYMENTS_LAST_ROUTE_KEY, pathname);
  } catch {
    // Storage can be unavailable in private or native WebView contexts.
  }
}

export type NetworkFeePortalNotice = {
  code:
    | "loadTimeout"
    | "loginTimeout"
    | "formTimeout"
    | "navigationFailed"
    | "credentialsUnavailable";
  message: string;
};
