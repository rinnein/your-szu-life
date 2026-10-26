/**
 * 给 `<m-tabs>` 内部的 tablist 起可访问名称。
 *
 * material 把 `role="tablist"` 放在宿主的 shadow root 里，宿主既没有 role，
 * 也没有接受可访问名称的属性，因此 `aria-label` 只能写到内部元素上。
 *
 * React 的 ref 回调早于元素升级：material 的 React 包装在布局阶段才注册标签，
 * 那一刻 `<m-tabs>` 还没有 shadow root。升级完成后宿主会派发一次
 * `connected` 之外没有专门的信号，因此这里用有限次重试等待 shadow root 出现
 * （约一帧内完成，最多 20 帧），不会造成持续轮询。
 *
 * 这是普通函数而非 hook：调用点位置各不相同，其中一些在提前 return 之后。
 */
export function nameTablist(host: HTMLElement | null, label: string): void {
  if (!host) return;

  let frames = 0;
  const apply = () => {
    const list = host.shadowRoot?.querySelector('[role="tablist"]');
    if (list) {
      if (list.getAttribute("aria-label") !== label) list.setAttribute("aria-label", label);
      return;
    }
    if (frames++ < 20) requestAnimationFrame(apply);
  };
  apply();
}
