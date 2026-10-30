import { createFileRoute } from "@tanstack/react-router";
import EnrollmentLayout from "../../enrollment/EnrollmentLayout";
import { WebvpnAuthProvider } from "../../enrollment/webvpn-gate";

/**
 * WebVPN 认证闸门只服务选课模块，因此挂在选课布局内部而不是应用根节点。
 */
function EnrollmentRoute() {
  return (
    <WebvpnAuthProvider>
      <EnrollmentLayout />
    </WebvpnAuthProvider>
  );
}

export const Route = createFileRoute("/study/enrollment")({
  validateSearch: (s: Record<string, unknown>): { view?: "cache" } => ({
    view: s.view === "cache" ? "cache" : undefined,
  }),
  component: EnrollmentRoute,
});
