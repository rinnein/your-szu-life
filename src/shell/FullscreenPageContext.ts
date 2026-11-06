import { createContext } from "react";

/** 全屏页面挂载时注册，清理时恢复导航；不依赖 WebView 的 CSS :has() 支持。 */
export const FullscreenPageContext = createContext<(() => () => void) | null>(null);
