import { Card, Button } from "material/react";
import { useEffect, useState } from "react";
import { PageShell } from "../components";
import { useEnrollment } from "./use-enrollment";
import { ModuleHeader, OperationError } from "./shared";
import type { Probe } from "./model";
export default function WebvpnPage() {
  const { snapshot, portal, run, pending, errors } = useEnrollment();
  const [probe, setProbe] = useState<Probe>();
  useEffect(() => {
    if (snapshot?.message === "认证成功")
      void run<Probe[]>({ type: "probe" }).then((p) =>
        setProbe(p?.find((p) => p.mode === "webvpn")),
      );
  }, [snapshot?.message, run]);
  return (
    <PageShell>
      <div className="mx-auto grid w-full max-w-3xl gap-6">
        <ModuleHeader title="WebVPN" description="校外访问与学校额外认证" />
        <Card
          variant="outlined"
          headline="连接校园服务"
          subhead="在学校专属页面完成2FA认证后，可访问 WebVPN 下的校园系统。"
        >
          <div className="p-4 pt-0">
            <p className="text-muted">
              {probe?.needs2fa
                ? "需要完成2FA验证"
                : probe?.latency != null
                  ? `会话有效 · 延迟 ${probe.latency} ms`
                  : probe?.message || "尚未验证当前会话"}
            </p>
            <OperationError message={errors.probe || errors["portal-webvpn"]} />
          </div>
          <div slot="actions" className="flex flex-wrap gap-3">
            <Button disabled={!!pending["portal-webvpn"]} onClick={() => void portal("webvpn")}>
              打开专属2FA认证
            </Button>
            <Button
              variant="tonal"
              disabled={!!pending.probe}
              onClick={() =>
                void run<Probe[]>({ type: "probe" }).then((p) =>
                  setProbe(p?.find((p) => p.mode === "webvpn")),
                )
              }
            >
              验证当前会话
            </Button>
          </div>
        </Card>
      </div>
    </PageShell>
  );
}
