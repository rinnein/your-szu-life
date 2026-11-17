/** 底栏选中药丸的位置与尺寸（相对 `.app-nav-bar` 的 padding box）。 */
export interface NavigationIndicatorBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface RectLike {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * 由 material shadow root 里的指示器 rect 换算药丸相对浮层的位置。
 *
 * `origin` 是 `.app-nav-bar` 的 border box：药丸是绝对定位在 shell 的
 * padding box 上的，因此要扣掉上边框与左边框（底栏目前只有上边框）。
 * 宿主还没升级、或页面全屏隐藏导致 rect 全为 0 时返回 `null`，调用方据此
 * 不渲染药丸 —— 否则它会停在左上角。
 */
export function navigationIndicatorBox(
  target: RectLike,
  origin: RectLike,
  offset: { borderTop: number; borderLeft: number } = { borderTop: 0, borderLeft: 0 },
): NavigationIndicatorBox | null {
  if (!(target.width > 0) || !(target.height > 0)) return null;
  return {
    left: target.left - origin.left - offset.borderLeft,
    top: target.top - origin.top - offset.borderTop,
    width: target.width,
    height: target.height,
  };
}
