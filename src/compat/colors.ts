import { converter, formatRgb, parse } from "culori";
import { mixProperty } from "./contracts";

const rgb = converter("rgb");
const oklab = converter("oklab");

/** CSS Color 5：透明色参与预乘 alpha 插值，避免把透明黑混成灰色。 */
export function evaluateColorMix(value: string, currentColor = "black"): string | undefined {
  const direct = parse(value);
  if (direct) return formatRgb(rgb(direct));
  const match = /^color-mix\(\s*in\s+(srgb|oklab)\s*,\s*(.+)\)$/i.exec(value);
  if (!match) return undefined;
  const parts = match[2].split(",");
  // rgb()/rgba() 自己也含逗号；只在括号外切分。
  let depth = 0;
  let split = -1;
  for (let i = 0; i < match[2].length; i++) {
    const char = match[2][i];
    if (char === "(") depth++;
    else if (char === ")") depth--;
    else if (char === "," && depth === 0) {
      split = i;
      break;
    }
  }
  if (parts.length < 2 || split < 0) return undefined;
  const read = (text: string) => {
    const weight = /\s+([\d.]+)%\s*$/.exec(text);
    const color = parse(
      text
        .replace(/\s+[\d.]+%\s*$/, "")
        .trim()
        .replace(/currentcolor/gi, currentColor),
    );
    return { color, weight: weight ? Number(weight[1]) / 100 : undefined };
  };
  const a = read(match[2].slice(0, split));
  const b = read(match[2].slice(split + 1));
  if (!a.color || !b.color) return undefined;
  const wa = a.weight ?? (b.weight === undefined ? 0.5 : 1 - b.weight);
  const wb = b.weight ?? 1 - wa;
  const sum = wa + wb;
  if (wa < 0 || wb < 0 || sum <= 0) return undefined;
  const aa = a.color.alpha ?? 1;
  const ab = b.color.alpha ?? 1;
  const alpha = (wa * aa + wb * ab) / sum;
  if (!alpha) return "rgba(0, 0, 0, 0)";
  const inOklab = match[1].toLowerCase() === "oklab";
  const ca = inOklab ? oklab(a.color)! : rgb(a.color)!;
  const cb = inOklab ? oklab(b.color)! : rgb(b.color)!;
  const channels = ca.mode === "oklab" ? (["l", "a", "b"] as const) : (["r", "g", "b"] as const);
  const mixed: Record<string, number | string> = { mode: ca.mode, alpha: alpha * Math.min(sum, 1) };
  for (const channel of channels) {
    const av = (ca as unknown as Record<string, number>)[channel] ?? 0;
    const bv = (cb as unknown as Record<string, number>)[channel] ?? 0;
    mixed[channel] = (av * wa * aa + bv * wb * ab) / (sum * alpha);
  }
  return formatRgb(rgb(mixed as unknown as Parameters<typeof rgb>[0]));
}

export function rgbProperty(role: string): string {
  return `--szu-compat-rgb-${role}`;
}

/** 主题色与透明色的混合可直接使用 rgba(var(--rgb), alpha)，无需 DOM 观察。 */
export function compatibilityThemeTokens(tokens: Record<string, string>): Record<string, string> {
  const result = { ...tokens };
  for (const [property, value] of Object.entries(tokens)) {
    if (!property.startsWith("--mtrl-sys-color-")) continue;
    const parsed = parse(value);
    const color = parsed && rgb(parsed);
    if (color)
      result[rgbProperty(property.slice("--mtrl-sys-color-".length))] = [color.r, color.g, color.b]
        .map((n) => Math.round(n * 255))
        .join(", ");
  }
  // 首帧预生成应用使用的双角色混色；组件局部变量由所在作用域补齐。
  const pairs: [string, number, string][] = [
    ["success", 16, "surface"],
    ["success", 18, "surface"],
    ["warning", 18, "surface"],
    ["warning", 20, "surface"],
    ["info", 16, "surface"],
    ["info", 18, "surface"],
    ["tertiary", 20, "surface"],
    ["on-surface", 8, "surface-container-high"],
    ["on-tertiary-container", 8, "tertiary-container"],
    ...Array.from({ length: 12 }, (_, i): [string, number, string] => [
      `accent-${String(i + 1).padStart(2, "0")}`,
      20,
      "surface",
    ]),
  ];
  for (const [a, weight, b] of pairs) {
    const source = `color-mix(in srgb,var(--mtrl-sys-color-${a}) ${weight}%,var(--mtrl-sys-color-${b}))`;
    const value = evaluateColorMix(
      `color-mix(in srgb,${tokens[`--mtrl-sys-color-${a}`]} ${weight}%,${tokens[`--mtrl-sys-color-${b}`]})`,
    );
    if (value) result[mixProperty(source)] = value;
  }
  return result;
}
