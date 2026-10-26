import { act, within, type queries } from "@testing-library/react";

/**
 * material 组件把角色与可访问名称放在 shadow root 内部，宿主要素本身
 * 没有 role，因此 `screen.getByRole()` 查不到它们。
 *
 * 用法：
 *   const { host, inner } = shadowOf(container, "m-button");
 *   expect(inner.getByRole("button")).toBeEnabled();
 *
 * 文本内容仍在 light DOM 的 <slot> 里，所以 `screen.getByText()` 照旧可用。
 */
export function shadowOf(
  root: HTMLElement | Document,
  tag: string,
): {
  host: HTMLElement;
  inner: ReturnType<typeof within<typeof queries>>;
} {
  const host = (root instanceof Document ? root.body : root).querySelector<HTMLElement>(tag);
  if (!host) throw new Error(`找不到 <${tag}>`);
  if (!host.shadowRoot) throw new Error(`<${tag}> 没有 shadow root`);
  return { host, inner: within(host.shadowRoot as unknown as HTMLElement) };
}

/** 所有同类宿主的 shadow root 查询器，按 DOM 顺序。 */
export function allShadows(
  root: HTMLElement | Document,
  tag: string,
): ReturnType<typeof within<typeof queries>>[] {
  const base = root instanceof Document ? root.body : root;
  return Array.from(base.querySelectorAll<HTMLElement>(tag)).map((host) => {
    if (!host.shadowRoot) throw new Error(`<${tag}> 没有 shadow root`);
    return within(host.shadowRoot as unknown as HTMLElement);
  });
}

/** 等待自定义元素升级完成（注册发生在首次挂载后的一帧内）。 */
export async function waitForUpgrade(host: HTMLElement): Promise<void> {
  if (host.shadowRoot) return;
  await new Promise<void>((resolve) => {
    const check = () => (host.shadowRoot ? resolve() : requestAnimationFrame(check));
    check();
  });
}

/**
 * 往输入控件里真正键入文本。
 *
 * `fireEvent.change(host, ...)` 只会给 material 的宿主要素设一个 value
 * 参数，不会进入 shadow root 里的真实 `<input>`，也不会派发元素自己的
 * input 事件。这里定位内部控件、赋值并派发 input 事件，等价于用户键入。
 *
 * 对原生 `<input>`/`<textarea>` 直接操作，因此同一个辅助函数在迁移前后
 * 的测试里都适用。
 */
export function typeInto(field: HTMLElement, value: string): void {
  const target =
    field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement
      ? field
      : (field.shadowRoot?.querySelector("input, textarea") as
          | HTMLInputElement
          | HTMLTextAreaElement
          | null);
  if (!target) throw new Error("字段内没有可输入的控件");
  // material 在重渲染时会重建 shadow 内的输入控件：调用方在若干次 await
  // 之后持有的引用可能已经脱离文档，继续对它派发事件不会生效。
  // 此时按相同的可访问名称重新定位。
  let resolved = target;
  if (!resolved.isConnected) {
    const label = resolved.getAttribute("aria-label");
    const placeholder = resolved.getAttribute("placeholder");
    const escaped = (value: string) => value.replace(/"/g, '\\"');
    const fresh =
      (label && document.querySelector<HTMLInputElement>(`[aria-label="${escaped(label)}"]`)) ||
      (placeholder &&
        document.querySelector<HTMLInputElement>(`[placeholder="${escaped(placeholder)}"]`));
    if (fresh) resolved = fresh;
  }
  // 包在 act 里：直接派发事件不会被 React 的批处理刷新，
  // 紧随其后的断言会读到旧状态（Testing Library 的 fireEvent 正是这么做的）。
  act(() => {
    setNativeValue(resolved, value);
    resolved.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
    resolved.dispatchEvent(new Event("change", { bubbles: true, composed: true }));
    const rootNode = resolved.getRootNode();
    if (rootNode instanceof ShadowRoot && rootNode.host) {
      rootNode.host.dispatchEvent(
        new CustomEvent("input", { detail: { value }, bubbles: true, composed: true }),
      );
      rootNode.host.dispatchEvent(
        new CustomEvent("change", { detail: { value }, bubbles: true, composed: true }),
      );
    }
  });
}

/**
 * 写入值时不走元素自己的 `value` setter，而是走原型上的原生 setter。
 *
 * React 会在受控输入控件上装自己的 setter 来跟踪值：若用 `node.value = v`，
 * 跟踪器立刻记为新值，随后派发的 input 事件被判定为「值没变」，`onChange`
 * 就不会触发。绕过跟踪器后 React 才会读到真正的变化。
 */
function setNativeValue(field: HTMLInputElement | HTMLTextAreaElement, value: string): void {
  const proto =
    field instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(field, value);
}

/**
 * 取出控件所属的自定义元素（宿主）。
 *
 * material 的可见文本留在 light DOM 里，由 shadow 内的 `<slot>` 承接，
 * 而 `getByRole` 命中的是 shadow 内部的真实控件 —— 它的 `textContent`
 * 看不到被 slot 承接的文本。断言文案时要用宿主元素。
 * 原生元素不属于任何 shadow root，原样返回。
 */
export function hostOf(element: Element | null): Element | null {
  if (!element) return null;
  const root = element.getRootNode();
  return root instanceof ShadowRoot ? root.host : element;
}
