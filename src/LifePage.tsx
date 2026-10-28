import { FeatureGrid, type Feature } from "./components";
import { PageSurface } from "./shell/AppShell";
import { TopAppBar } from "./shell/TopAppBar";
import { AuthGate } from "./shell/AuthGate";
import { FaWallet, FaWifi } from "react-icons/fa6";

const LIFE_FEATURES: readonly Feature[] = [
  {
    label: "校园网登录",
    icon: FaWifi,
    description: "教工区 + 宿舍区 | 自动登录",
    to: "/life/campus-network",
  },
  {
    label: "校园卡/电费/网费",
    icon: FaWallet,
    description: "快速导航入口",
    to: "/life/campus-payments",
  },
];

export default function LifePage() {
  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <div className="grid gap-12">
          <TopAppBar title="生活" description="校园网与校园卡相关功能" showBack={false} />
          <FeatureGrid features={LIFE_FEATURES} />
        </div>
      </PageSurface>
    </AuthGate>
  );
}
