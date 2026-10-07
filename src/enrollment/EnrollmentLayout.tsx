import { useEffect, useRef, useState } from "react";
import { Outlet, useLocation } from "@tanstack/react-router";
import { Button, Chip, Spinner } from "@heroui/react";
import {
  LuBookOpen,
  LuCalendarDays,
  LuCheckCheck,
  LuListOrdered,
  LuSettings,
} from "react-icons/lu";
import { PageShell } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { useEnrollment } from "./use-enrollment";
import { Choice, ModuleHeader, OperationError } from "./shared";
import { enrollmentPages } from "./navigation";
import { useCacheMode, cachePath, livePath, canSelect } from "./view-mode";
import { OnlineUsers } from "./SchoolInfo";
import type { Snapshot } from "./model";
const icons = [LuBookOpen, LuCheckCheck, LuListOrdered, LuCalendarDays, LuSettings];
export default function EnrollmentLayout() {
  const cacheOnly = useCacheMode();
  const { snapshot, run, errors, official, pending } = useEnrollment(cacheOnly);
  const location = useLocation();
  const nav = useLogicalNavigation();
  const [checked, setChecked] = useState(false);
  const [unavailable, setUnavailable] = useState("");
  const account = useRef<string | undefined>(undefined);
  const mounted = useRef(true);
  const currentLocation = useRef(location);
  currentLocation.current = location;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const login = location.pathname === "/enrollment/login";
  const path = location.pathname.replace(/\/$/, "");
  const verify = () => {
    if (cacheOnly) {
      setChecked(true);
      return;
    }
    setUnavailable("");
    void run<{ status: string; message?: string; snapshot: Snapshot }>({ type: "session" }).then(
      (result) => {
        if (!mounted.current) return;
        setChecked(true);
        if (!result) return;
        if (result.status === "required" || result.status === "restricted") {
          const target = currentLocation.current.href;
          void nav.replace(`/enrollment/login?returnTo=${encodeURIComponent(target)}`);
        } else if (result.status === "unavailable")
          setUnavailable(result.message ?? "学校暂时无法连接，可继续查看缓存");
      },
    );
  };
  useEffect(() => {
    if (cacheOnly) {
      account.current = undefined;
      setChecked(true);
      return;
    }
    if (!snapshot) return;
    if (login) {
      account.current = snapshot.context.student;
      return;
    }
    if (account.current === snapshot.context.student) return;
    account.current = snapshot.context.student;
    verify();
  }, [snapshot?.context.student, login, cacheOnly]);
  const change = (target: string) => {
    if (target === path) return;
    if (target === "/enrollment") {
      void nav.back();
      return;
    }
    const destination = cacheOnly ? cachePath(target) : target;
    if (path === "/enrollment") void nav.push(destination);
    else void nav.replace(destination);
  };
  if (login) return <Outlet />;
  const page = enrollmentPages.find((p) => p.path === path) ?? enrollmentPages[0];
  return (
    <PageShell>
      <div className="mx-auto grid w-full max-w-[1440px] min-w-0 gap-5">
        {!cacheOnly && snapshot && (
          <OnlineUsers account={snapshot.context.student} mode={snapshot.mode} />
        )}
        <ModuleHeader
          title={page.label}
          backLabel={path === "/enrollment" ? "返回工作台" : "返回选课"}
          description="选课工作台"
          actions={
            <>
              <Chip size="sm" variant="soft">
                {cacheOnly
                  ? "只读缓存"
                  : !checked
                    ? "验证会话中"
                    : snapshot?.loggedIn
                      ? "已登录"
                      : "未登录"}{" "}
                · {snapshot?.mode === "webvpn" ? "WebVPN" : "校内访问"}
              </Chip>
              {cacheOnly && (
                <Button
                  size="sm"
                  variant="secondary"
                  onPress={() =>
                    void nav.replace(
                      `/enrollment/login?returnTo=${encodeURIComponent(livePath(location.href))}`,
                    )
                  }
                >
                  退出缓存模式
                </Button>
              )}
              {snapshot?.loggedIn && !cacheOnly && (
                <Button
                  size="sm"
                  variant="secondary"
                  isPending={!!pending.official}
                  onPress={() => void official()}
                >
                  打开学校官网
                </Button>
              )}
              {snapshot && !canSelect(snapshot) && !cacheOnly && (
                <Button
                  size="sm"
                  variant="secondary"
                  onPress={() =>
                    void nav.replace(
                      `/enrollment/login?returnTo=${encodeURIComponent(location.href)}`,
                    )
                  }
                >
                  登录选课
                </Button>
              )}
              {snapshot?.running && (
                <Button size="sm" variant="secondary" onPress={() => change("/enrollment/tasks")}>
                  后台任务运行中
                </Button>
              )}
            </>
          }
        />
        <OperationError
          message={errors.snapshot || errors.session || errors.official || unavailable}
          onRetry={verify}
        />
        {cacheOnly && (
          <p className="text-sm text-muted">
            仅浏览本地缓存，不自动连接选课系统。未缓存内容不会作为无课或未选课的依据。
          </p>
        )}
        <div className="lg:hidden">
          <Choice
            label="选课导航"
            value={path}
            onChange={change}
            items={enrollmentPages.map((p) => ({ id: p.path, label: p.label }))}
          />
        </div>
        <div className="grid min-w-0 gap-6 lg:grid-cols-[184px_minmax(0,1fr)]">
          <aside
            className="hidden self-start rounded-2xl bg-surface p-2 lg:block"
            aria-label="选课导航"
          >
            {enrollmentPages.map((p, i) => {
              const Icon = icons[i];
              return (
                <Button
                  key={p.path}
                  className="mb-1 w-full justify-start"
                  variant={path === p.path ? "primary" : "ghost"}
                  onPress={() => change(p.path)}
                >
                  <Icon />
                  {p.label}
                </Button>
              );
            })}
            <div className="mt-4 border-t border-separator pt-3">
              <Button
                className="w-full justify-start"
                variant="ghost"
                onPress={() => void nav.push("/academic")}
              >
                学业完成查询
              </Button>
            </div>
          </aside>
          <section className="min-w-0">
            {!cacheOnly && !snapshot && !checked && !errors.snapshot ? (
              <Spinner aria-label="正在连接选课服务" />
            ) : (
              <Outlet />
            )}
          </section>
        </div>
      </div>
    </PageShell>
  );
}
