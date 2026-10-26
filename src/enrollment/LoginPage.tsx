import { useEffect, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { Card, LoadingIndicator, Button } from "material/react";
import { Banner } from "../ui";
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
import { OcrModelRequiredDialog } from "../OcrModelGate";
import { useOcrModelStatus } from "../OcrModelSettings";
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
  const [ocrRequired, setOcrRequired] = useState(false);
  const [ocrDismissed, setOcrDismissed] = useState(false);
  const ocrStatus = useOcrModelStatus();
  const done = (value: Snapshot | undefined) => {
    if (value?.loggedIn) setShowCaptcha(false);
    if (canSelect(value)) void nav.replace(safeReturn(returnTo));
  };
  useEffect(() => {
    setShowCaptcha(false);
  }, [s?.context.student, s?.mode]);
  const login = () => {
    if (!ocrStatus || ocrStatus.status === "ready") setShowCaptcha(true);
    else {
      setOcrDismissed(false);
      setOcrRequired(true);
    }
  };
  useEffect(() => {
    if (
      ocrStatus &&
      ocrStatus.status !== "ready" &&
      ocrStatus.status !== "checking" &&
      !ocrDismissed
    ) {
      setOcrRequired(true);
    }
  }, [ocrStatus?.status, ocrDismissed]);
  useEffect(() => {
    if (ocrStatus?.status === "ready" && ocrRequired) {
      setOcrRequired(false);
      setShowCaptcha(true);
    }
  }, [ocrStatus?.status, ocrRequired]);
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
        <LoadingIndicator size={24} aria-label="正在打开只读缓存" />
      </PageShell>
    );
  return (
    <PageShell>
      <OcrModelRequiredDialog
        status={ocrStatus}
        isOpen={ocrRequired}
        onOpenChange={(open) => {
          setOcrRequired(open);
          if (!open) setOcrDismissed(true);
        }}
        onSuccess={() => {
          setOcrRequired(false);
          setShowCaptcha(true);
        }}
      />
      <div className="mx-auto grid w-full max-w-2xl gap-7">
        <ModuleHeader title="登录选课系统" description="选择访问方式，进入本科选课工作台" />
        <Card
          variant="outlined"
          headline="连接学校选课系统"
          subhead="优先恢复已有会话。WebVPN 额外认证在学校官方窗口完成。"
        >
          <div className="grid gap-5 p-4 pt-0">
            <AccessModes autoSelect={!showCaptcha} />
            {s && (
              <>
                <BatchInfo account={s.context.student} mode={s.mode} />
                <OnlineUsers account={s.context.student} mode={s.mode} />
              </>
            )}
            {s?.loggedIn && !canSelect(s) && (
              <Banner status="warning">
                <Banner.Title>登录成功，当前暂不可选课</Banner.Title>
                <Banner.Description>
                  {s.admissionMessage || "正在确认当前批次准入状态"}
                </Banner.Description>
                <div className="mt-3 flex flex-wrap gap-2">
                  <Button
                    size="xs"
                    variant="tonal"
                    onClick={() => void nav.replace(cachePath(safeReturn(returnTo)))}
                  >
                    查看只读缓存
                  </Button>
                  <Button
                    size="xs"
                    variant="tonal"
                    disabled={!!pending.session}
                    onClick={() =>
                      void run<{ status: string; snapshot: Snapshot }>({ type: "session" }).then(
                        (v) => v && done(v.snapshot),
                      )
                    }
                  >
                    重新检查准入
                  </Button>
                  <Button
                    size="xs"
                    variant="tonal"
                    disabled={!!pending.official}
                    onClick={() => void official()}
                  >
                    打开学校官网
                  </Button>
                </div>
              </Banner>
            )}
            <OperationError message={errors.snapshot || errors.session} />
            <OperationError message={errors.official} />
            <Button
              size="m"
              disabled={!s || showCaptcha || !!pending.login || !!pending.solve}
              onClick={login}
            >
              <LuLogIn />
              {s?.loggedIn && !canSelect(s) ? "重新登录" : "登录并进入选课"}
            </Button>
            {!!pending.login && (
              <p className="text-center text-sm text-muted">正在获取验证码并验证登录…</p>
            )}
            <Button
              variant="tonal"
              onClick={() => void nav.replace(cachePath(safeReturn(returnTo)))}
            >
              不登录，查看只读缓存
            </Button>
            {showCaptcha && s && (
              <CaptchaInput
                key={`${s.context.student}:${s.mode}`}
                onAuthenticated={done}
                onModelRequired={() => {
                  setOcrDismissed(false);
                  setOcrRequired(true);
                }}
              />
            )}
          </div>
        </Card>
      </div>
    </PageShell>
  );
}
