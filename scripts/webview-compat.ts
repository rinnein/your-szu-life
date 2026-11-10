import { readFile } from "node:fs/promises";
import postcss from "postcss";
import presetEnv from "postcss-preset-env";
import selectorParser from "postcss-selector-parser";
import { selectorSpecificity } from "@csstools/selector-specificity";
import valueParser from "postcss-value-parser";
import { compatId, mixProperty, SHEET_METADATA } from "../src/compat/contracts";

const FALLBACK_EASING = "cubic-bezier(0.2, 0, 0, 1)";
const transformProperties = new Set(["translate", "rotate", "scale", "transform"]);
const compositeTransform =
  "var(--szu-translate, translate(0)) var(--szu-rotate, rotate(0deg)) var(--szu-scale, scale(1)) var(--szu-transform, translate(0))";

function appendScoped(destination: any, clone: any, source: any) {
  let scoped = clone;
  for (let parent = source.parent; parent && parent.type !== "root"; parent = parent.parent) {
    if (parent.type !== "atrule") continue;
    const wrapper = parent.clone({ nodes: [] });
    wrapper.append(scoped);
    scoped = wrapper;
  }
  destination.append(scoped);
}

function candidateFor(node: any) {
  let subject = node;
  let selector = subject.parent;
  while (selector.parent?.type === "pseudo" && selector.parent.value !== ":has") {
    subject = selector.parent;
    selector = subject.parent;
  }
  const index = selector.nodes.indexOf(subject);
  const end = selector.nodes.findIndex(
    (item: any, i: number) => i > index && item.type === "combinator",
  );
  const candidate = selector.clone();
  candidate.nodes = candidate.nodes.slice(0, end < 0 ? undefined : end);
  candidate.walkPseudos((pseudo: any) => {
    if (
      pseudo.value === ":has" ||
      pseudo.toString().includes(":has(") ||
      pseudo.toString().includes("[data-szu-has-")
    )
      pseudo.remove();
    else if (pseudo.value.startsWith("::")) pseudo.remove();
  });
  return candidate.toString().trim() || "*";
}

function hasSelector(selector: string, metadata: any) {
  const ast = selectorParser().astSync(selector);
  const nodes: any[] = [];
  ast.walkPseudos((node: any) => {
    if (node.value === ":has") nodes.push(node);
  });
  // 内层优先，Material 部分选择器包含嵌套 :has；外层引用内层状态属性。
  for (const node of nodes.reverse()) {
    const specificity = selectorSpecificity(node);
    const conditions = node.nodes.map((child: any) => child.toString());
    const attribute = `data-szu-has-${compatId(conditions.join(","))}`;
    const candidate = candidateFor(node);
    metadata.has.push({ attribute, candidate, conditions });
    let replacement = specificity.b ? `[${attribute}]` : `:where([${attribute}])`;
    for (const [prefix, count] of [
      ["#", specificity.a],
      [".", Math.max(0, specificity.b - 1)],
      ["", specificity.c],
    ] as const) {
      for (let i = 0; i < count; i++) replacement += `:not(${prefix}szu-compat-specificity-${i})`;
    }
    node.replaceWith(...selectorParser().astSync(replacement).first.nodes);
  }
  return ast.toString();
}

function subjects(selector: string) {
  return selectorParser()
    .astSync(selector)
    .nodes.map((node) => {
      const ast = node.clone();
      let pseudo = "";
      ast.walkPseudos((item) => {
        if (["::before", "::after", "::backdrop", "::selection"].includes(item.value)) {
          pseudo = item.value;
          item.remove();
        }
      });
      return { selector: ast.toString(), pseudo };
    })
    .filter((item) => !item.selector.includes("::"));
}

function transparentRoleMix(node: any) {
  const text = valueParser.stringify(node);
  const match =
    /^color-mix\(\s*in srgb\s*,\s*var\(--mtrl-sys-color-([\w-]+)\)\s+([\d.]+)%\s*,\s*transparent\s*\)$/.exec(
      text,
    );
  return match ? `rgba(var(--szu-compat-rgb-${match[1]}), ${Number(match[2]) / 100})` : undefined;
}

/** 同一管线用于普通 CSS 与 Material 的 JS 注册样式。 */
export function compatibilityCss() {
  return {
    postcssPlugin: "szu-webview-compat",
    Once(root: any) {
      const metadata: { has: any[]; mix: any[]; containers: any[]; owners: any[] } = {
        has: [],
        mix: [],
        containers: [],
        owners: [],
      };
      const fallbackTransforms = postcss.atRule({ name: "supports", params: "not (scale: 1)" });
      const fallbackEasing = postcss.atRule({
        name: "supports",
        params: "not (transition-timing-function: linear(0, 1))",
      });
      const originalRules: any[] = [];
      root.walkRules((rule: any) => originalRules.push(rule));
      for (const rule of originalRules) {
        const keyframe = rule.parent?.name?.endsWith("keyframes");
        if (!keyframe && rule.selector.includes(":has(")) {
          const fallback = rule.clone({ selector: hasSelector(rule.selector, metadata) });
          const supports = postcss.atRule({ name: "supports", params: "not selector(:has(*))" });
          supports.append(fallback);
          rule.before(supports);
        }
      }
      // 包括新插入的 :has 回退，使局部颜色与回退选择器具有一致作用域。
      root.walkRules((rule: any) => {
        const keyframe = rule.parent?.name?.endsWith("keyframes");
        const declarations = rule.nodes.filter((node: any) => node.type === "decl");
        for (const declaration of declarations) {
          if (declaration.value.includes("color-mix(")) {
            const ast = valueParser(declaration.value);
            const sources: { source: string; expression: string; target: string }[] = [];
            ast.walk((node: any) => {
              if (node.type !== "function" || node.value !== "color-mix") return;
              const expression = valueParser.stringify(node);
              const alpha = transparentRoleMix(node);
              if (alpha) {
                node.type = "word";
                node.value = alpha;
                delete node.nodes;
                return false;
              }
              // 静态表达式交给 Lightning CSS / preset-env；动态表达式保留来源。
              if (!/var\(|currentColor/i.test(expression)) return;
              const target = mixProperty(expression);
              const source = target.replace("-color-", "-source-");
              sources.push({ source, expression, target });
              node.type = "word";
              node.value = `var(${target}, transparent)`;
              delete node.nodes;
              return false;
            });
            const fallback = ast.toString();
            if (fallback !== declaration.value)
              declaration.before(declaration.clone({ value: fallback }));
            for (const { source, expression, target } of sources) {
              rule.append(postcss.decl({ prop: source, value: expression }));
              if (!keyframe)
                for (const subject of subjects(rule.selector))
                  metadata.mix.push({ ...subject, source, target });
            }
          }
          if (/\b\d*\.?\d+(cqw|dvh)\b/.test(declaration.value)) {
            const value = declaration.value
              .replace(
                /([\d.]+)cqw\b/g,
                (_: string, n: string) => `(${Number(n) / 100} * var(--szu-content-width, 100vw))`,
              )
              .replace(
                /([\d.]+)dvh\b/g,
                (_: string, n: string) =>
                  `(${Number(n) / 100} * var(--szu-viewport-height, 100vh))`,
              );
            declaration.before(
              declaration.clone({
                value: declaration.value.includes("calc(") ? value : `calc(${value})`,
              }),
            );
          }
          if (["container", "container-name"].includes(declaration.prop)) {
            const name = declaration.value.split("/")[0].trim();
            if (name !== "none") metadata.owners.push({ selector: rule.selector, name });
          }
          if (declaration.value.includes("linear(")) {
            const ast = valueParser(declaration.value);
            ast.walk((node: any) => {
              if (node.type === "function" && node.value === "linear") {
                node.type = "word";
                node.value = FALLBACK_EASING;
                delete node.nodes;
                return false;
              }
            });
            // 写在独立 @supports 中，不改变现代浏览器的曲线。
            const clone = postcss.rule({ selector: rule.selector });
            clone.append(declaration.clone({ value: ast.toString() }));
            if (!keyframe) appendScoped(fallbackEasing, clone, rule);
          }
        }
        if (keyframe) return;
        const transforms = declarations.filter((d: any) => transformProperties.has(d.prop));
        const transitions = declarations.filter(
          (d: any) =>
            /^(transition|transition-property)$/.test(d.prop) &&
            /\b(scale|translate|rotate)\b/.test(d.value),
        );
        if (transforms.length || transitions.length) {
          const clone = postcss.rule({ selector: rule.selector });
          for (const d of transforms) {
            const value =
              d.value === "none"
                ? // @ts-expect-error
                  {
                    scale: "scale(1)",
                    translate: "translate(0)",
                    rotate: "rotate(0deg)",
                    transform: "translate(0)",
                  }[d.prop]
                : d.prop === "transform"
                  ? d.value
                  : `${d.prop}(${d.value.trim().replace(/\s+/g, ", ")})`;
            clone.append(postcss.decl({ prop: `--szu-${d.prop}`, value, important: d.important }));
          }
          // 普通 transform（尤其 none）保持原状，避免给原本没有包含块的容器
          // 强加 identity transform；只有独立变换需要组合回退。
          if (transforms.some((d: any) => d.prop !== "transform"))
            clone.append(postcss.decl({ prop: "transform", value: compositeTransform }));
          for (const d of transitions)
            clone.append(
              d.clone({
                value: d.value
                  .replace(/\b(scale|translate|rotate)\b/g, "transform")
                  .replace(/linear\([^)]*\)/g, FALLBACK_EASING),
              }),
            );
          appendScoped(fallbackTransforms, clone, rule);
        }
      });
      // @container 不可静态降为 @media：由真实容器宽度驱动属性。
      root.walkAtRules("container", (container: any) => {
        const match = /^([\w-]+)\s+\((min|max)-width:\s*([\d.]+)px\)$/.exec(container.params);
        if (!match)
          throw container.error(`Unsupported compatibility container query: ${container.params}`);
        const attribute = `data-szu-container-${compatId(container.params)}`;
        metadata.containers.push({ name: match[1], attribute, [match[2]]: Number(match[3]) });
        const clone = container.clone();
        clone.walkRules((rule: any) => {
          rule.selector = selectorParser()
            .astSync(rule.selector)
            .nodes.map((selector) => `:where([${attribute}]) ${selector}`)
            .join(",");
        });
        const supports = postcss.atRule({
          name: "supports",
          params: "not (container-type: inline-size)",
        });
        supports.append(clone.nodes);
        container.after(supports);
      });
      if (fallbackEasing.nodes?.length) root.append(fallbackEasing);
      if (fallbackTransforms.nodes?.length) {
        // 这些变量必须是元素自己的值，不能继承祖先的位移或缩放。
        const reset = postcss.rule({ selector: ":where(*, ::before, ::after)" });
        for (const [property, value] of Object.entries({
          translate: "translate(0)",
          rotate: "rotate(0deg)",
          scale: "scale(1)",
          transform: "translate(0)",
        }))
          reset.append(postcss.decl({ prop: `--szu-${property}`, value }));
        const defaults = postcss.atRule({ name: "layer", params: "szu.compat-defaults" });
        defaults.append(
          postcss.atRule({ name: "supports", params: "not (scale: 1)" }).append(reset),
        );
        root.prepend(defaults);
        root.append(fallbackTransforms);
      }
      if (Object.values(metadata).some((entries: any[]) => entries.length)) {
        for (const key of Object.keys(metadata) as (keyof typeof metadata)[])
          metadata[key] = [
            ...new Map(metadata[key].map((entry: any) => [JSON.stringify(entry), entry])).values(),
          ];
        const marker = postcss.rule({ selector: ":root" });
        marker.append(
          postcss.decl({
            prop: SHEET_METADATA,
            value: `"${Buffer.from(JSON.stringify(metadata)).toString("base64")}"`,
          }),
        );
        root.append(marker);
      }
    },
  };
}

export function compatibilityPostcssPlugins() {
  return [
    compatibilityCss(),
    presetEnv({
      browsers: ["Chrome >= 90", "Edge >= 90", "Firefox >= 104", "Safari >= 16"],
      stage: 3,
      features: {
        "cascade-layers": true,
        "custom-properties": false,
        "has-pseudo-class": false,
        "color-mix": false,
      },
    }),
  ];
}

export function materialCompatibilityStyles() {
  return {
    name: "szu-material-compatibility-styles",
    enforce: "pre",
    async transform(code: string, id: string) {
      const path = id.split("?")[0].replaceAll("\\", "/");
      if (path.endsWith("/material/dist/components/snackbar/layer.js")) {
        // Material 的 dialog 均以 showModal 打开；旧内核不认识 :modal，
        // 用 open 判定其状态，现代浏览器继续使用原生伪类判定。
        return {
          code: code.replace(
            /node\.matches\((['"]):modal\1\)/g,
            '(typeof CSS !== "undefined" && typeof CSS.supports === "function" && CSS.supports("selector(:modal)") ? node.matches(":modal") : node.open)',
          ),
          map: null,
        };
      }
      if (
        !/\/material\/dist\/elements\/css\/(?:hosts\/)?[\w-]+\.js$/.test(path) ||
        !code.includes("registerStyles(")
      )
        return;
      // 使用旁置原始 CSS，避免 eval 上游模块或猜测字符串边界。
      const name = /registerStyles\(\{\s*"([^"]+)"\s*:/.exec(code)?.[1];
      if (!name) throw new Error(`Unrecognized Material style registration: ${path}`);
      const cssPath = path.replace(/\.js$/, ".css");
      const css = await readFile(cssPath, "utf8");
      const { css: transformed } = await postcss(compatibilityPostcssPlugins()).process(css, {
        from: cssPath,
      });
      const start = code.indexOf("registerStyles(");
      return {
        code:
          code.slice(0, start) +
          `registerStyles({${JSON.stringify(name)}:${JSON.stringify(transformed)}});\n`,
        map: null,
      };
    },
    // 无需生成文件；插件只转换模块，依赖安装与升级仍由包管理器维护。
  };
}
