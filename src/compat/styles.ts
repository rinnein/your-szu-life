import { evaluateColorMix } from "./colors";
import { SHEET_METADATA, type CompatSheet, type HasRule } from "./contracts";

export interface CompatibilityFeatures {
  has: boolean;
  colorMix: boolean;
  containers: boolean;
  viewport: boolean;
}
export function supportsCss(value: string): boolean {
  try {
    return typeof CSS !== "undefined" && CSS.supports(value);
  } catch {
    return false;
  }
}
export function detectCompatibilityFeatures(): CompatibilityFeatures {
  return {
    has: !supportsCss("selector(:has(*))"),
    colorMix: !supportsCss("color: color-mix(in srgb, black, white)"),
    containers: !supportsCss("container-type: inline-size") || !supportsCss("width: 1cqw"),
    viewport: !supportsCss("height: 1dvh"),
  };
}

function matchesRelative(element: Element, condition: string): boolean {
  try {
    const trimmed = condition.trim();
    if (trimmed.startsWith("+"))
      return element.nextElementSibling?.matches(trimmed.slice(1).trim()) ?? false;
    if (trimmed.startsWith("~")) {
      let sibling = element.nextElementSibling;
      while (sibling) {
        if (sibling.matches(trimmed.slice(1).trim())) return true;
        sibling = sibling.nextElementSibling;
      }
      return false;
    }
    return !!element.querySelector(trimmed.startsWith(">") ? `:scope ${trimmed}` : trimmed);
  } catch {
    return false;
  }
}
export function synchronizeHas(root: Document | ShadowRoot, rules: HasRule[]): void {
  // 同一条件可能用于 html、body 和多个控件；清理须针对候选集合的并集。
  const groups = new Map<string, { conditions: string[]; candidates: Set<string> }>();
  for (const rule of rules) {
    let group = groups.get(rule.attribute);
    if (!group) {
      group = { conditions: rule.conditions, candidates: new Set() };
      groups.set(rule.attribute, group);
    }
    group.candidates.add(rule.candidate);
  }
  for (const [attribute, rule] of groups) {
    const candidates = new Set(
      Array.from(rule.candidates).flatMap((selector) => query(root, selector)),
    );
    for (const element of candidates) {
      const matches = rule.conditions.some((condition) => matchesRelative(element, condition));
      if (element.hasAttribute(attribute) !== matches) element.toggleAttribute(attribute, matches);
    }
    // 移除之前匹配、如今已不再属于候选集合的状态。
    for (const element of query(root, `[${attribute}]`)) {
      if (!candidates.has(element)) element.removeAttribute(attribute);
    }
  }
}
function query(root: Document | ShadowRoot, selector: string): Element[] {
  try {
    if (selector === ":host" && root instanceof ShadowRoot) return [root.host];
    // 宿主 CSS 的 :host(...) 在浏览器查询 API 中不是合法选择器。
    if (selector.startsWith(":host") && root instanceof ShadowRoot) {
      const match = /^:host\(([^)]+)\)$/.exec(selector);
      return match && root.host.matches(match[1]) ? [root.host] : [];
    }
    return Array.from(root.querySelectorAll(selector)).filter(
      (element) => element.getRootNode() === root,
    );
  } catch {
    return [];
  }
}

interface RootState {
  root: Document | ShadowRoot;
  observer: MutationObserver;
  colorStyle?: HTMLStyleElement;
  pseudoColors: Map<string, string>;
}

/** 不补丁 querySelector/matches；只维护构建时明确列出的状态与颜色。 */
export function installStyleCompatibility(
  document: Document,
  features: CompatibilityFeatures,
): { refresh: () => void; flush: () => void; dispose: () => void; rootCount: () => number } {
  const roots = new Map<Document | ShadowRoot, RootState>();
  const dirty = new Set<RootState>();
  let disposed = false;
  let frame = 0;
  let sequence = 0;
  const ids = new WeakMap<Element, string>();
  const inlineColors = new WeakMap<HTMLElement, Map<string, string>>();
  let sheetCache = new WeakMap<CSSStyleSheet, CompatSheet[]>();
  const widths = new Map<Element, { state: RootState; name: string; width: number }>();
  const resize =
    typeof ResizeObserver === "function"
      ? new ResizeObserver((entries) => {
          for (const entry of entries) {
            const owner = widths.get(entry.target);
            if (!owner) continue;
            owner.width = entry.contentRect.width;
            schedule(owner.state);
          }
        })
      : undefined;

  function schedule(state: RootState) {
    if (disposed) return;
    dirty.add(state);
    if (!frame) frame = requestAnimationFrame(flush);
  }
  function metadata(state: RootState): CompatSheet[] {
    const sheets = [
      ...Array.from(state.root.styleSheets ?? []),
      ...Array.from(state.root.adoptedStyleSheets ?? []),
    ];
    const result: CompatSheet[] = [];
    for (const sheet of sheets) {
      let cached = sheetCache.get(sheet);
      if (!cached) {
        cached = [];
        const walk = (rules: CSSRuleList) => {
          for (const rule of Array.from(rules)) {
            const style = (rule as CSSStyleRule).style;
            const marker = style
              ?.getPropertyValue(SHEET_METADATA)
              .trim()
              .replace(/^['"]|['"]$/g, "");
            if (marker) {
              try {
                cached!.push(JSON.parse(atob(marker)) as CompatSheet);
              } catch {
                /* 非本插件的标记不参与。 */
              }
            }
            const nested = (rule as CSSGroupingRule).cssRules;
            if (nested) walk(nested);
          }
        };
        try {
          walk(sheet.cssRules);
        } catch {
          /* 不读取跨域样式表。 */
        }
        sheetCache.set(sheet, cached);
      }
      result.push(...cached);
    }
    return result;
  }
  function flush() {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    if (disposed) return;
    for (const [root, state] of roots) {
      if (root instanceof ShadowRoot && !root.host.isConnected) {
        state.observer.disconnect();
        state.colorStyle?.remove();
        roots.delete(root);
        dirty.delete(state);
      }
    }
    for (const [element] of widths) {
      if (!element.isConnected) {
        resize?.unobserve(element);
        widths.delete(element);
      }
    }
    const pending = Array.from(dirty);
    dirty.clear();
    for (const state of pending) {
      const sheets = metadata(state);
      if (features.has)
        synchronizeHas(
          state.root,
          sheets.flatMap((sheet) => sheet.has),
        );
      if (features.containers) {
        for (const owner of sheets.flatMap((sheet) => sheet.owners)) {
          for (const element of query(state.root, owner.selector)) {
            if (!widths.has(element)) {
              widths.set(element, {
                state,
                name: owner.name,
                width: element.getBoundingClientRect().width,
              });
              resize?.observe(element);
            }
          }
        }
        for (const element of query(state.root, ".app-content")) {
          if (!widths.has(element)) {
            widths.set(element, {
              state,
              name: "app-content",
              width: element.getBoundingClientRect().width,
            });
            resize?.observe(element);
          }
        }
        const conditions = sheets.flatMap((sheet) => sheet.containers);
        for (const [element, owner] of widths) {
          if (owner.state !== state) continue;
          if (owner.name === "app-content") {
            const value = `${owner.width}px`;
            if (document.documentElement.style.getPropertyValue("--szu-content-width") !== value)
              document.documentElement.style.setProperty("--szu-content-width", value);
          }
          for (const condition of conditions) {
            if (owner.name !== condition.name) continue;
            const matches =
              (condition.min === undefined || owner.width >= condition.min) &&
              (condition.max === undefined || owner.width <= condition.max);
            if (element.hasAttribute(condition.attribute) !== matches)
              element.toggleAttribute(condition.attribute, matches);
          }
        }
      }
      if (features.colorMix) {
        const seen = new Set<string>();
        for (const rule of sheets.flatMap((sheet) => sheet.mix)) {
          for (const element of query(state.root, rule.selector)) {
            if (!(element instanceof HTMLElement)) continue;
            let id = ids.get(element);
            if (!id) {
              id = String(++sequence);
              ids.set(element, id);
            }
            const key = `${id}${rule.pseudo}:${rule.target}`;
            if (seen.has(key)) continue;
            seen.add(key);
            const computed = getComputedStyle(element, rule.pseudo || null);
            const source = computed.getPropertyValue(rule.source).trim();
            if (!source) continue;
            const value = evaluateColorMix(source, computed.color);
            if (!value) continue;
            if (!rule.pseudo || rule.pseudo === "::backdrop") {
              let colors = inlineColors.get(element);
              if (!colors) {
                colors = new Map();
                inlineColors.set(element, colors);
              }
              if (colors.get(rule.target) !== value) {
                colors.set(rule.target, value);
                element.style.setProperty(rule.target, value);
              }
            } else {
              element.setAttribute("data-szu-color-node", id);
              state.pseudoColors.set(
                key,
                `[data-szu-color-node="${id}"]${rule.pseudo}{${rule.target}:${value}}`,
              );
            }
          }
        }
        for (const key of state.pseudoColors.keys())
          if (!seen.has(key)) state.pseudoColors.delete(key);
        if (state.pseudoColors.size || state.colorStyle) {
          if (!state.colorStyle) {
            state.colorStyle = document.createElement("style");
            state.colorStyle.dataset.szuCompatColors = "";
            (state.root instanceof ShadowRoot ? state.root : document.head).append(
              state.colorStyle,
            );
          }
          const text = Array.from(state.pseudoColors.values()).join("");
          if (state.colorStyle.textContent !== text) state.colorStyle.textContent = text;
        }
      }
    }
  }
  function refresh() {
    sheetCache = new WeakMap();
    for (const state of roots.values()) schedule(state);
  }
  const events = [
    "input",
    "change",
    "focusin",
    "focusout",
    "pointerdown",
    "pointerup",
    "pointerover",
    "pointerout",
    "keydown",
    "keyup",
    "toggle",
    "slotchange",
    "load",
  ];
  function track(root: Document | ShadowRoot) {
    if (roots.has(root) || disposed) return;
    const observer = new MutationObserver((records) => {
      const relevant = records.filter((record) => {
        if (record.type === "attributes" && record.attributeName?.startsWith("data-szu-"))
          return false;
        if (
          record.target instanceof HTMLElement &&
          record.target.hasAttribute("data-szu-compat-colors")
        )
          return false;
        // 忽略兼容层自己的 inline custom property 写入，避免无限 RAF 循环。
        if (
          record.type === "attributes" &&
          record.attributeName === "style" &&
          record.target instanceof HTMLElement
        ) {
          const strip = (text: string) => text.replace(/--szu-compat-color-[^:;]+:[^;]*;?/g, "");
          if (strip(record.oldValue ?? "") === strip(record.target.getAttribute("style") ?? ""))
            return false;
        }
        return true;
      });
      if (!relevant.length) return;
      if (
        relevant.some(
          (record) =>
            record.target instanceof HTMLStyleElement ||
            Array.from(record.addedNodes).some((node) => node instanceof HTMLStyleElement),
        )
      )
        sheetCache = new WeakMap();
      discover(root);
      if (root === document) {
        // 主题与宿主属性会跨 Shadow DOM 继承，局部观察器看不到这些变化。
        for (const state of roots.values()) schedule(state);
      } else schedule(roots.get(root)!);
    });
    const state: RootState = { root, observer, pseudoColors: new Map() };
    roots.set(root, state);
    observer.observe(root, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeOldValue: true,
    });
    for (const event of events) root.addEventListener(event, refresh, true);
    schedule(state);
  }
  function discover(root: Document | ShadowRoot) {
    for (const element of root.querySelectorAll("*")) {
      if (element.shadowRoot && !roots.has(element.shadowRoot)) {
        track(element.shadowRoot);
        discover(element.shadowRoot);
      }
    }
  }
  const originalAttach = Element.prototype.attachShadow;
  const patchedAttach: typeof originalAttach = function (this: Element, init) {
    const root = originalAttach.call(this, init);
    if (init.mode === "open") track(root);
    return root;
  };
  Element.prototype.attachShadow = patchedAttach;
  const updateViewport = () => {
    if (features.viewport)
      document.documentElement.style.setProperty(
        "--szu-viewport-height",
        `${window.visualViewport?.height ?? window.innerHeight}px`,
      );
    refresh();
  };
  window.addEventListener("resize", updateViewport);
  window.visualViewport?.addEventListener("resize", updateViewport);
  track(document);
  discover(document);
  updateViewport();
  return {
    refresh,
    flush,
    rootCount: () => roots.size,
    dispose() {
      disposed = true;
      if (frame) cancelAnimationFrame(frame);
      if (Element.prototype.attachShadow === patchedAttach)
        Element.prototype.attachShadow = originalAttach;
      for (const state of roots.values()) {
        state.observer.disconnect();
        state.colorStyle?.remove();
        for (const event of events) state.root.removeEventListener(event, refresh, true);
      }
      roots.clear();
      dirty.clear();
      widths.clear();
      resize?.disconnect();
      window.removeEventListener("resize", updateViewport);
      window.visualViewport?.removeEventListener("resize", updateViewport);
    },
  };
}
