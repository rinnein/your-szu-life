import { useEffect, useRef, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, motion } from "motion/react";
import { TransformComponent, TransformWrapper } from "react-zoom-pan-pinch";
import { IconButton } from "material/react";
import {
  closeImageZoom,
  getImageZoomSnapshot,
  subscribeImageZoom,
  type ImageZoomRequest,
} from "./image-zoom";
import { effects } from "../motion/tokens";

/**
 * 全屏图片查看浮层：react-zoom-pan-pinch 负责滚轮/双指缩放、双击切换与
 * 拖拽平移，这里只负责壳——遮罩、关闭按钮、入场退场、Escape 与滚动锁。
 *
 * 关闭路径：右上角按钮、Escape（Android 系统返回经 android-back 对
 * role="dialog" 的 Escape 兜底也落到这条路径）、fit 档位点击图片盒外的
 * 空白区域。点击图片盒内不关闭（保住双击缩放），放大后空白点击也不关闭
 * （避免平移取景时误退）；这两种情况用关闭按钮或返回键。
 */

const CLOSE_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>';

/** fit 档位的判定余量：onTransform 的 scale 与 1 之间允许浮点误差。 */
const FIT_SCALE = 1.001;

/**
 * 浮层点击关闭的判定：点到关闭按钮或图片盒内不关（后者保住双击缩放），
 * 放大后（scale > fit）空白点击也不关，避免平移取景时误退。
 */
export function shouldCloseOnScrimClick(scale: number, target: Element | null): boolean {
  if (target?.closest("button") || target?.closest(".oj-image-zoom-content")) return false;
  return scale <= FIT_SCALE;
}

function ImageZoomOverlay({ request }: { request: ImageZoomRequest }) {
  const scaleRef = useRef(1);

  useEffect(() => {
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeImageZoom();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  return (
    <motion.div
      className="oj-image-zoom"
      role="dialog"
      aria-modal="true"
      aria-label="查看题目图片"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={effects.default}
      onClick={(event) => {
        if (shouldCloseOnScrimClick(scaleRef.current, event.target as HTMLElement | null))
          closeImageZoom();
      }}
    >
      <TransformWrapper
        minScale={1}
        maxScale={6}
        centerOnInit
        doubleClick={{ mode: "toggle", step: 2.5 }}
        onTransform={(_, state) => {
          scaleRef.current = state.scale;
        }}
      >
        <TransformComponent
          wrapperClass="oj-image-zoom-viewport"
          contentClass="oj-image-zoom-content"
        >
          <img src={request.src} alt={request.alt ?? ""} className="oj-image-zoom-image" />
        </TransformComponent>
      </TransformWrapper>
      <IconButton
        className="oj-image-zoom-close"
        aria-label="关闭图片查看"
        icon={CLOSE_ICON}
        onClick={closeImageZoom}
      />
    </motion.div>
  );
}

export function ImageZoomHost() {
  const request = useSyncExternalStore(subscribeImageZoom, getImageZoomSnapshot, () => undefined);
  if (typeof document === "undefined") return null;
  return createPortal(
    <AnimatePresence>
      {request && <ImageZoomOverlay key="oj-image-zoom" request={request} />}
    </AnimatePresence>,
    document.body,
  );
}
