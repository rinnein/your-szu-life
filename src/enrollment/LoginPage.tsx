import { useEffect, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { Button, Card, Spinner, Alert } from "@heroui/react";
import { LuLogIn } from "react-icons/lu";
import { PageShell } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { useEnrollment } from "./use-enrollment";
import { ModuleHeader, OperationError } from "./shared";
import { BatchInfo, OnlineUsers } from "./SchoolInfo";
import CaptchaInput from "./CaptchaInput";
import AccessModes from "./AccessModes";
import { cachePath, canSelect, useCacheMode } from "./view-mode";
import type { Snapshot } from "./model";
export function safeReturn(path?: string) {
  return path?.startsWith("/enrollment") &&
    !path.startsWith("/enrollment/login") &&
    [
      "/enrollment",
      "/enrollment/selected",
      "/enrollment/tasks",
      "/enrollment/timetable",
      "/enrollment/settings",
    ].includes(path.split("?")[0])
    ? path
    : "/enrollment";
}
export default function LoginPage() {
  const cacheOnly = useCacheMode();
  const { snapshot: s, run, pending, errors, official } = useEnrollment(cacheOnly);
  const nav = useLogicalNavigation();
  const { returnTo } = useSearch({ from: "/enrollment/login" });
  const [showCaptcha, setShowCaptcha] = useState(false);
  const done = (value: Snapshot | undefined) => {
    if (value?.loggedIn) setShowCaptcha(false);
    if (canSelect(value)) void nav.replace(safeReturn(returnTo));
  };
  useEffect(() => {
    setShowCaptcha(false);
  }, [s?.context.student, s?.mode]);
  const login = () => setShowCaptcha(true);
  useEffect(() => {
    let live = true;
    if (cacheOnly) {
      void nav.replace(cachePath("/enrollment"));
      return;
    }
    void run<{ status: string }>({ type: "session" }).then((v) => {
      if (live && v?.status === "ready") void nav.replace(safeReturn(returnTo));
    });
    return () => {
      live = false;
    };
  }, []);
  if (cacheOnly)
    return (
      <PageShell>
        <Spinner aria-label="正在打开只读缓存" />
      </PageShell>
    );
  return (
    <PageShell>
      <div className="mx-auto grid w-full max-w-2xl gap-7">
        <ModuleHeader title="登录选课系统" description="选择访问方式，进入本科选课工作台" />
        <Card>
          <Card.Header>
            <Card.Title>连接学校选课系统</Card.Title>
            <Card.Description>
              优先恢复已有会话。WebVPN 额外认证在学校官方窗口完成。
            </Card.Description>
          </Card.Header>
          <Card.Content className="grid gap-5">
            <AccessModes autoSelect={!showCaptcha} />
            {s && (
              <>
                <BatchInfo account={s.context.student} mode={s.mode} />
                <OnlineUsers account={s.context.student} mode={s.mode} />
              </>
            )}
            {s?.loggedIn && !canSelect(s) && (
              <Alert status="warning">
                <Alert.Content>
                  <Alert.Title>登录成功，当前暂不可选课</Alert.Title>
                  <Alert.Description>
                    {s.admissionMessage || "正在确认当前批次准入状态"}
                  </Alert.Description>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="secondary"
                      onPress={() => void nav.replace(cachePath(safeReturn(returnTo)))}
                    >
                      查看只读缓存
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      isPending={!!pending.session}
                      onPress={() =>
                        void run<{ status: string; snapshot: Snapshot }>({ type: "session" }).then(
                          (v) => v && done(v.snapshot),
                        )
                      }
                    >
                      重新检查准入
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      isPending={!!pending.official}
                      onPress={() => void official()}
                    >
                      打开学校官网
                    </Button>
                  </div>
                </Alert.Content>
              </Alert>
            )}
            <OperationError message={errors.snapshot || errors.session} />
            <OperationError message={errors.official} />
            <Button
              size="lg"
              isDisabled={!s || showCaptcha}
              isPending={!!pending.login || !!pending.solve}
              onPress={login}
            >
              <LuLogIn />
              {s?.loggedIn && !canSelect(s) ? "重新登录" : "登录并进入选课"}
            </Button>
            {!!pending.login && (
              <p className="text-center text-sm text-muted">正在获取验证码并验证登录…</p>
            )}
            <Button
              variant="secondary"
              onPress={() => void nav.replace(cachePath(safeReturn(returnTo)))}
            >
              不登录，查看只读缓存
            </Button>
            {showCaptcha && s && (
              <CaptchaInput key={`${s.context.student}:${s.mode}`} onAuthenticated={done} />
            )}
          </Card.Content>
        </Card>
      </div>
    </PageShell>
  );
}
