import { useLogicalNavigation } from "./logical-navigation";
import { Outlet, useLocation } from "@tanstack/react-router";
import { useEffect } from "react";
import { LoadingIndicator, Tab, Tabs } from "material/react";
import { useAuth } from "./auth";
import { CAMPUS_PAYMENTS_ROUTES, saveLastCampusPaymentsRoute } from "./campus-payments";
import { ModuleHeader, PageShell } from "./components";
import { nameTablist } from "./ui/tablist";

/**
 * 校园服务的四个入口。
 *
 * 它们是同一逻辑页面内的内容切换（各自只是不同的服务面板），因此用 tab 的
 * 语义表达，并用 `replace` 同步 URL——不制造新的返回层级。
 */
const SERVICES = [
  { path: "/life/campus-payments/card", label: "校园卡" },
  { path: "/life/campus-payments/network", label: "网费" },
  { path: "/life/campus-payments/electricity", label: "查电量" },
  { path: "/life/campus-payments/recharge", label: "充电费" },
] as const;

export default function CampusPaymentsPage() {
  const { state } = useAuth();
  const navigation = useLogicalNavigation();
  const location = useLocation();
  const authenticated = "Authenticated" in state;
  const checking = "Checking" in state;

  useEffect(() => {
    if (!authenticated && !checking) void navigation.back();
  }, [authenticated, checking, navigation]);

  useEffect(() => {
    if ((CAMPUS_PAYMENTS_ROUTES as readonly string[]).includes(location.pathname)) {
      saveLastCampusPaymentsRoute(location.pathname);
    }
  }, [location.pathname]);

  if (!authenticated) {
    if (checking)
      return (
        <PageShell centered>
          <LoadingIndicator size={40} aria-label="正在检查统一认证状态" />
        </PageShell>
      );
    return null;
  }

  const active = SERVICES.find((s) => s.path === location.pathname)?.path ?? SERVICES[0].path;

  return (
    <PageShell maxWidth="xl">
      <div className="grid gap-8">
        <ModuleHeader
          title="校园卡/电费/网费"
          eyebrow="CAMPUS SERVICES"
          description="官方服务入口与宿舍电费查询；不会自动充值、扣款或修改套餐"
        />

        <Tabs
          ref={(host) => nameTablist(host, "校园服务分类")}
          value={active}
          onChange={(event) => {
            if (event.detail.value) void navigation.replace(event.detail.value);
          }}
        >
          {SERVICES.map((service) => (
            <Tab key={service.path} value={service.path}>
              {service.label}
            </Tab>
          ))}
        </Tabs>

        <Outlet />
      </div>
    </PageShell>
  );
}
