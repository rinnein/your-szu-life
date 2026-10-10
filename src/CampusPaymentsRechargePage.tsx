import { LuZap } from "react-icons/lu";
import { WeChatServiceCard } from "./WeChatServiceCard";
import { Section } from "./components";

export default function CampusPaymentsRechargePage() {
  return (
    <Section title="充电费" description="通过官方微信服务办理宿舍电费充值">
      <WeChatServiceCard service="electricity" icon={<LuZap aria-hidden="true" />} />
    </Section>
  );
}
