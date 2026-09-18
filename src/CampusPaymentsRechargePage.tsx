import { LuZap } from "react-icons/lu";
import { WeChatServiceCard } from "./WeChatServiceCard";

export default function CampusPaymentsRechargePage() {
  return (
    <div className="electricity-recharge">
      <CardTitle />
      <WeChatServiceCard service="electricity" icon={<LuZap aria-hidden="true" />} />
    </div>
  );
}

function CardTitle() {
  return (
    <header className="payments-section-heading">
      <h2>充电费</h2>
      <p>通过官方微信服务办理宿舍电费充值。</p>
    </header>
  );
}
