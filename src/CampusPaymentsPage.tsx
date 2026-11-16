import { useLogicalNavigation } from "./logical-navigation";
import { Outlet, useLocation } from "@tanstack/react-router";
import { useEffect } from "react";
import { LoadingIndicator, Tab, Tabs } from "material/react";
import { useAuth } from "./auth";
import {
  CAMPUS_PAYMENTS_ROUTES,
  CAMPUS_PAYMENTS_SERVICES,
  saveLastCampusPaymentsRoute,
} from "./campus-payments";
import { ModuleHeader, PageShell } from "./components";
import { nameTablist } from "./ui/tablist";

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

  const active =
    CAMPUS_PAYMENTS_SERVICES.find((s) => s.path === location.pathname)?.path ??
    CAMPUS_PAYMENTS_SERVICES[0].path;

  return (
    <PageShell maxWidth="xl">
      <ModuleHeader
        title="校园卡/电费/网费"
        eyebrow="CAMPUS SERVICES"
        description="官方服务入口与宿舍电费查询；不会自动充值、扣款或修改套餐"
      />

      <div className="grid gap-8">
        <Tabs
          ref={(host) => nameTablist(host, "校园服务分类")}
          value={active}
          onChange={(event) => {
            if (event.detail.value)
              void navigation.replace(event.detail.value, { resetScroll: false });
          }}
        >
          {CAMPUS_PAYMENTS_SERVICES.map((service) => (
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
