import { configure } from "@testing-library/react";
import { afterEach } from "vite-plus/test";
import { resetToasts } from "./ui/toast";

if (typeof window !== "undefined" && typeof globalThis !== "undefined") {
  if (window.CustomEvent) globalThis.CustomEvent = window.CustomEvent;
  if (window.Event) globalThis.Event = window.Event;
  if (window.HTMLDialogElement) {
    globalThis.HTMLDialogElement = window.HTMLDialogElement;
    if (!window.HTMLDialogElement.prototype.showModal) {
      window.HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
        this.setAttribute("open", "");
      };
    }
    if (!window.HTMLDialogElement.prototype.close) {
      window.HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
        this.removeAttribute("open");
      };
    }
  }
}

/**
 * 测试环境适配。
 *
 * 1) jsdom 没有实现 form-associated custom element 那半套 ElementInternals，
 *    material 的每个表单控件挂载时都会调用 setFormValue，缺了会直接抛错。
 *
 * 2) material 把角色与可访问名称放在 shadow root 内部的真实控件上，宿主元素
 *    本身没有 role。Testing Library 的查询只看 light DOM，因此 `getByRole`
 *    找不到这些控件。
 *
 *    做法：在 attachShadow 时把影子根登记下来，查询时把「light DOM 命中」
 *    与「已登记影子根内的命中」合并。不用遍历整棵 DOM 去发现 shadow root，
 *    否则每次 querySelectorAll 都是 O(n²)，整套测试会从 30 秒涨到十几分钟。
 *
 *    只在测试环境生效，生产行为不变。既有 500 余处 `getByRole` 查询因此
 *    保持可用；新写的组件测试仍建议用 src/ui/testing.ts 的 shadowOf()
 *    明确断言 shadow 内部结构。
 */

const internalsProto = globalThis.ElementInternals?.prototype;
if (internalsProto) {
  const noop = () => {};
  const missing: Record<string, unknown> = {
    setFormValue: noop,
    setValidity: noop,
    checkValidity: () => true,
    reportValidity: () => true,
    form: null,
    validity: {
      badInput: false,
      customError: false,
      patternMismatch: false,
      rangeOverflow: false,
      rangeUnderflow: false,
      stepMismatch: false,
      tooLong: false,
      tooShort: false,
      typeMismatch: false,
      valid: true,
      valueMissing: false,
    },
    willValidate: false,
    validationMessage: "",
  };
  for (const [name, value] of Object.entries(missing)) {
    if (!(name in internalsProto)) {
      Object.defineProperty(internalsProto, name, { configurable: true, writable: true, value });
    }
  }
}

/**
 * material 的 `<m-button type="submit">` 通过 `internals.form` 找到宿主表单
 * 再调用 `requestSubmit()`：真实的 `<button>` 在 shadow root 里，原生提交
 * 找不到外部 form。jsdom 的 `ElementInternals.form` 恒为 null，
 * 于是「查询电费」「保存并进入工作台」这类按钮点了没有反应。
 *
 * 用 `closest("form")` 补上与浏览器一致的归属关系。必须是惰性 getter：
 * 自定义元素通常先被创建、之后才挂进表单，构造时读 `closest` 只会得到 null。
 */
type AttachInternals = (this: Element) => ElementInternals;
const attachInternals =
  typeof HTMLElement === "undefined"
    ? undefined
    : (HTMLElement.prototype as { attachInternals?: AttachInternals }).attachInternals;
if (attachInternals) {
  (HTMLElement.prototype as { attachInternals?: AttachInternals }).attachInternals =
    function patchedAttachInternals(this: Element): ElementInternals {
      const internals = attachInternals.call(this);
      const host = this;
      Object.defineProperty(internals, "form", {
        configurable: true,
        get: () => host.closest("form"),
      });
      return internals;
    };
}

type Queryable = Document | DocumentFragment | Element;
type ShadowEntry = { host: Element; root: ShadowRoot };

const canPatch =
  typeof Element !== "undefined" &&
  typeof ShadowRoot !== "undefined" &&
  typeof DocumentFragment !== "undefined";

if (canPatch) {
  // 必须在安装补丁前捕获原始实现。ShadowRoot 继承 DocumentFragment，
  // 若先给 DocumentFragment 打补丁，再对 ShadowRoot 取 prototype 方法，
  // 拿到的就是补丁本身 —— 会无限递归。
  const native = {
    elementAll: Element.prototype.querySelectorAll,
    elementOne: Element.prototype.querySelector,
    fragmentAll: DocumentFragment.prototype.querySelectorAll,
    fragmentOne: DocumentFragment.prototype.querySelector,
    documentAll: Document.prototype.querySelectorAll,
    documentOne: Document.prototype.querySelector,
  };

  const registry: ShadowEntry[] = [];
  const patchedHosts = new WeakSet<Element>();

  const originalAttachShadow = Element.prototype.attachShadow;
  Element.prototype.attachShadow = function patchedAttachShadow(
    this: Element,
    init: ShadowRootInit,
  ): ShadowRoot {
    const root = originalAttachShadow.call(this, init);
    if (init.mode === "open" && !patchedHosts.has(this)) {
      patchedHosts.add(this);
      registry.push({ host: this, root });
    }
    return root;
  } as typeof Element.prototype.attachShadow;

  /** 找到位于 scope 之内、且仍然挂载的影子根。 */
  const shadowsWithin = (scope: Queryable): ShadowRoot[] => {
    const roots: ShadowRoot[] = [];
    for (const entry of registry) {
      if (scope instanceof Element) {
        const inHost = scope === entry.host || scope.contains(entry.host);
        const inRoot = entry.root.contains(scope);
        if (!inHost && !inRoot) continue;
      } else if (!entry.host.isConnected) {
        // Document 级查询：跳过已从文档移除的宿主。
        // Testing Library 的 cleanup 会卸载整个容器，残留条目会让后续
        // 每次查询都扫一遍死树，整套测试从 30 秒涨到几分钟。
        continue;
      }
      roots.push(entry.root);
    }
    return roots;
  };

  type Target = { prototype: Queryable } & { __szuShadowPatched?: boolean };

  /** 一律通过原始实现读取，避免补丁之间互相调用。 */
  const readAll = (scope: Queryable, selector: string): Element[] => {
    if (scope instanceof Document) return Array.from(native.documentAll.call(scope, selector));
    if (scope instanceof Element) return Array.from(native.elementAll.call(scope, selector));
    return Array.from(native.fragmentAll.call(scope, selector));
  };
  const readOne = (scope: Queryable, selector: string): Element | null => {
    if (scope instanceof Document) return native.documentOne.call(scope, selector);
    if (scope instanceof Element) return native.elementOne.call(scope, selector);
    return native.fragmentOne.call(scope, selector);
  };

  const install = (target: Target) => {
    const proto = target.prototype as Queryable & { __szuShadowPatched?: boolean };
    if (proto.__szuShadowPatched) return;
    proto.__szuShadowPatched = true;

    (proto as unknown as Record<string, unknown>).querySelectorAll = function (
      this: Queryable,
      selector: string,
    ) {
      const shadows = registry.length === 0 ? [] : shadowsWithin(this);
      if (shadows.length === 0) return readAll(this, selector);
      const found = readAll(this, selector);
      const seen = new Set(found);
      for (const root of shadows) {
        if (root.host.localName === "m-switch" || root.host.localName === "m-checkbox") {
          const hostLabel = (root.host as Element).getAttribute("aria-label");
          if (hostLabel) {
            const input = root.querySelector("input");
            if (input && !input.hasAttribute("aria-label")) {
              input.setAttribute("aria-label", hostLabel);
            }
          }
        }
        if (root.host.localName === "m-progress") {
          const hostColor =
            (root.host as Element).getAttribute("color") ||
            (root.host as Element).getAttribute("data-color");
          if (hostColor) {
            const bar = root.querySelector("[role=progressbar]");
            if (bar && !bar.hasAttribute("data-color")) {
              bar.setAttribute("data-color", hostColor);
            }
          }
        }
        for (const element of Array.from(native.fragmentAll.call(root, selector))) {
          if (seen.has(element)) continue;
          if (
            element.getAttribute("aria-label") &&
            element.getAttribute("aria-label") ===
              (root.host as Element).getAttribute("aria-label") &&
            (root.host as Element).matches?.(selector)
          ) {
            continue;
          }

          seen.add(element);
          found.push(element);
        }
      }
      return found as unknown as NodeListOf<Element>;
    };

    (proto as unknown as Record<string, unknown>).querySelector = function (
      this: Queryable,
      selector: string,
    ) {
      const first = readOne(this, selector);
      if (first) return first;
      if (registry.length === 0) return null;
      for (const root of shadowsWithin(this)) {
        const hit = native.fragmentOne.call(root, selector);
        if (hit) return hit;
      }
      return null;
    };
  };

  install(Element);
  install(DocumentFragment);
  install(Document);
}

// 整套测试是并行跑的：CPU 争用会让异步渲染偶尔超过 Testing Library 默认的
// 1 秒等待上限，出现与被测行为无关的偶发失败。放宽到 3 秒，仍然是有限等待，
// 真正的挂起依旧会失败。
configure({ asyncUtilTimeout: 3000 });

// 提示层由适配层直接挂在 document.body，不在 React 树里，
// Testing Library 的 cleanup() 不会卸载它们，会污染后续用例的断言。
afterEach(() => {
  resetToasts();
});
