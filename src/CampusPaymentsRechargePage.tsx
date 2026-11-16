import { LuZap } from "react-icons/lu";
import { WeChatServiceCard } from "./WeChatServiceCard";

export default function CampusPaymentsRechargePage() {
  return <WeChatServiceCard service="electricity" icon={<LuZap aria-hidden="true" />} />;
}
