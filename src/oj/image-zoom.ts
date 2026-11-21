/**
 * 图片查看浮层的命令式桥：`Html` 组件的点击委托只负责发出请求，
 * 浮层实例由 `ImageZoomHost`（挂在 OJ 路由根部）统一渲染，整页最多一个。
 */

export type ImageZoomRequest = {
  src: string;
  alt?: string;
};

let current: ImageZoomRequest | undefined;
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

/** 打开图片查看浮层；重复调用直接替换当前内容。 */
export function openImageZoom(request: ImageZoomRequest): void {
  current = request;
  emit();
}

/** 关闭浮层；没有打开时是空操作。 */
export function closeImageZoom(): void {
  if (!current) return;
  current = undefined;
  emit();
}

export function subscribeImageZoom(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** useSyncExternalStore 快照：对象引用在两次请求之间保持稳定。 */
export function getImageZoomSnapshot(): ImageZoomRequest | undefined {
  return current;
}
