import { LuDownload } from "react-icons/lu";
import { useLogicalNavigation } from "../logical-navigation";

/** 巡课相关页面顶栏动作：进入录像下载管理页。 */
export function PatrolTopActions() {
  const navigation = useLogicalNavigation();
  return (
    <button
      type="button"
      data-webview-navigation
      className="top-app-bar-icon-button top-app-bar-downloads"
      aria-label="打开录像下载管理"
      title="录像下载管理"
      onClick={() => void navigation.push("/study/patrol/downloads")}
    >
      <LuDownload aria-hidden="true" size={20} />
    </button>
  );
}
