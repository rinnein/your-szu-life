import { schemeToTokens } from "material/core/theme";
import { generateSchemes } from "./scheme";
import type { SchemeVariant } from "./defaults";

export const THEME_NAME = "szu";
export const THEME_STYLE_ID = "szu-md3-theme";

/** material 的 schemeToTokens 只输出 THEME_ROLES，状态色需要单独补。 */
const STATUS_PROPERTIES = [
  "success",
  "on-success",
  "warning",
  "on-warning",
  "info",
  "on-info",
] as const;

function block(declarations: Record<string, string>): string {
  return Object.entries(declarations)
    .map(([property, value]) => `${property}:${value};`)
    .join("");
}

function withStatus(
  tokens: Record<string, string>,
  scheme: Record<string, string>,
): Record<string, string> {
  const merged = { ...tokens };
  for (const role of STATUS_PROPERTIES) {
    const value = scheme[role];
    if (value) merged[`--mtrl-sys-color-${role}`] = value;
  }
  return merged;
}

/** 生成 material 主题 CSS：`[data-theme]` 浅色 + `[data-theme-mode="dark"]` 深色。 */
export function buildThemeCss(seed: string, variant: SchemeVariant): string {
  const schemes = generateSchemes(seed, variant);
  const tokens = schemeToTokens({ light: schemes.light, dark: schemes.dark });
  const light = withStatus(tokens.light, schemes.light as unknown as Record<string, string>);
  const dark = withStatus(tokens.dark, schemes.dark as unknown as Record<string, string>);
  return (
    `[data-theme="${THEME_NAME}"]{${block(light)}}` +
    `[data-theme="${THEME_NAME}"][data-theme-mode="dark"]{${block(dark)}}`
  );
}

/**
 * 把主题写进文档。多次调用只替换同一 <style>，避免堆积。
 * 非浏览器环境（如 node 测试）静默跳过。
 */
export function applyThemeCss(seed: string, variant: SchemeVariant): void {
  if (typeof document === "undefined") return;
  let style = document.getElementById(THEME_STYLE_ID) as HTMLStyleElement | null;
  if (!style) {
    style = document.createElement("style");
    style.id = THEME_STYLE_ID;
    document.head.append(style);
  }
  style.textContent = buildThemeCss(seed, variant);
}
