import { AppLifecycleBridge } from "./app-lifecycle";
import { AndroidBackNavigation } from "./AndroidBackNavigation";
import { DeepLinkNavigation } from "./DeepLinkNavigation";
import { NotificationCenter } from "./notifications/NotificationCenter";
import { OjCompanionBridge } from "./oj/OjCompanionBridge";
import { OjProgressBridge } from "./oj/OjProgressBridge";

/**
 * 根级无界面桥接集合。
 *
 * 这些组件不渲染 UI，但都必须在整个应用生命周期内常驻，且与当前页面无关：
 * - AppLifecycleBridge：前后台焦点/可见性上报，以及回到前台时刷新 OJ 监测；
 * - AndroidBackNavigation / DeepLinkNavigation：系统返回键与深链接入口；
 * - NotificationCenter：应用内通知展示与通知点击跳转的唯一出口；
 * - OjProgressBridge：每个 WebView 都要同步原生共享的 OJ 进度缓存；
 * - OjCompanionBridge：CPH 伴侣通道只在主文档建立，但不能依赖用户停在 OJ 页。
 *
 * 集中成一个组件，根布局只表达「应用基础设施 + 内容」两层结构。
 */
export function AppBridges() {
  return (
    <>
      <AppLifecycleBridge />
      <AndroidBackNavigation />
      <DeepLinkNavigation />
      <NotificationCenter />
      <OjProgressBridge />
      <OjCompanionBridge />
    </>
  );
}
