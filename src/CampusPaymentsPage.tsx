import { Outlet, useLocation, useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { Button, CloseButton, Spinner } from "@heroui/react";
import { LuArrowLeft, LuCreditCard, LuWifi, LuZap } from "react-icons/lu";
import { useAuth } from "./auth";
import { CAMPUS_PAYMENTS_ROUTES, saveLastCampusPaymentsRoute } from "./campus-payments";
import { PageShell } from "./components";

export default function CampusPaymentsPage() {
  const { state } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const authenticated = "Authenticated" in state;
  const checking = "Checking" in state;
  useEffect(() => {
    if (!authenticated && !checking) void navigate({ to: "/", replace: true });
  }, [authenticated, checking, navigate]);
  useEffect(() => {
    if ((CAMPUS_PAYMENTS_ROUTES as readonly string[]).includes(location.pathname)) {
      saveLastCampusPaymentsRoute(location.pathname);
    }
  }, [location.pathname]);
  if (!authenticated) {
    if (checking)
      return (
        <PageShell centered>
          <Spinner size="lg" aria-label="正在检查统一认证状态" />
        </PageShell>
      );
    return null;
  }
  const current = location.pathname.split("/").pop();
  return (
    <PageShell>
      <div className="payments-layout">
        <CloseButton
          className="payments-back"
          aria-label="返回工作台"
          onPress={() => void navigate({ to: "/" })}
        >
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <header className="payments-header">
          <span className="eyebrow">CAMPUS SERVICES</span>
          <h1>校园卡/电费/网费</h1>
          <p className="page-description">
            官方服务入口与宿舍电费查询；不会自动充值、扣款或修改套餐。
          </p>
        </header>
        <nav className="payments-tabs" aria-label="校园服务分类">
          <Button
            variant={current === "card" ? "primary" : "outline"}
            onPress={() => void navigate({ to: "/campus-payments/card" })}
          >
            <LuCreditCard aria-hidden="true" />
            校园卡
          </Button>
          <Button
            variant={current === "network" ? "primary" : "outline"}
            onPress={() => void navigate({ to: "/campus-payments/network" })}
          >
            <LuWifi aria-hidden="true" />
            网费
          </Button>
          <Button
            variant={current === "electricity" ? "primary" : "outline"}
            onPress={() => void navigate({ to: "/campus-payments/electricity" })}
          >
            <LuZap aria-hidden="true" />
            查电量
          </Button>
          <Button
            variant={current === "recharge" ? "primary" : "outline"}
            onPress={() => void navigate({ to: "/campus-payments/recharge" })}
          >
            <LuZap aria-hidden="true" />
            充电费
          </Button>
        </nav>
        <Outlet />
      </div>
    </PageShell>
  );
}
