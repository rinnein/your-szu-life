import { detectCompatibilityFeatures, installStyleCompatibility } from "./styles";

let initialization: Promise<void> | undefined;

/** 必须在 Material 的静态依赖执行、注册自定义元素之前调用。 */
export function initializeCompatibility(): Promise<void> {
  return (initialization ??= (async () => {
    const features = detectCompatibilityFeatures();
    if (typeof HTMLElement.prototype.showPopover !== "function") {
      const polyfill = await import("@oddbird/popover-polyfill/fn");
      polyfill.apply({ layerName: "" });
      const { installPopoverPortals } = await import("./popover");
      installPopoverPortals((root) => polyfill.injectStyles(root, ""));
    }
    if (Object.values(features).some(Boolean)) {
      const styles = installStyleCompatibility(document, features);
      document.addEventListener("szu-theme-changed", styles.refresh);
    }
  })());
}
