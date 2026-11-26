import { AppLifecycleBridge } from "./app-lifecycle";
import { AnalyticsBridge } from "./AnalyticsBridge";
import { AndroidBackNavigation } from "./AndroidBackNavigation";
import { DeepLinkNavigation } from "./DeepLinkNavigation";
import { ModuleActivityBridge } from "./ModuleActivityBridge";
import { NotificationCenter } from "./notifications/NotificationCenter";
import { OjCompanionBridge } from "./oj/OjCompanionBridge";
import { OjProgressBridge } from "./oj/OjProgressBridge";
import { PortNoticeBridge } from "./PortNoticeBridge";
import { QrScannerHost } from "./qr-scan/QrScannerHost";

/**
 * 根级无界面桥接集合。
 *
 * 这些组件不渲染 UI，但都必须在整个应用生命周期内常驻，且与当前页面无关：
 * - AppLifecycleBridge：前后台焦点/可见性上报；
 * - AnalyticsBridge：匿名页面浏览上报，受调试页统计开关控制；
 * - ModuleActivityBridge：把当前功能页面上报给后端，用于动态轮询档位；
 * - AndroidBackNavigation / DeepLinkNavigation：系统返回键与深链接入口；
 * - NotificationCenter：应用内通知展示与通知点击跳转的唯一出口；
 * - PortNoticeBridge：本地服务端口切换/耗尽的模态框出口；
 * - OjProgressBridge：每个 WebView 都要同步原生共享的 OJ 进度缓存；
 * - OjCompanionBridge：CPH 伴侣通道只在主文档建立，但不能依赖用户停在 OJ 页；
 * - QrScannerHost：全屏扫码取景器的唯一出口，配对授权与签到共享共用。
 *
 * 集中成一个组件，根布局只表达「应用基础设施 + 内容」两层结构。
 */
export function AppBridges() {
  return (
    <>
      <AppLifecycleBridge />
      <AnalyticsBridge />
      <ModuleActivityBridge />
      <AndroidBackNavigation />
      <DeepLinkNavigation />
      <NotificationCenter />
      <PortNoticeBridge />
      <OjProgressBridge />
      <OjCompanionBridge />
      <QrScannerHost />
    </>
  );
}
