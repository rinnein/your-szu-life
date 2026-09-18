import { LuCreditCard } from "react-icons/lu";
import { WeChatServiceCard } from "./WeChatServiceCard";
export default function CampusPaymentsCardPage() {
  return <WeChatServiceCard service="campus-card" icon={<LuCreditCard aria-hidden="true" />} />;
}
