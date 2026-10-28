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

/** 两套模式下的系统底色（供读取/测试，不再由主题推给原生）。 */
export type ThemeSurface = { light: string; dark: string };

/** 生成 material 主题 CSS 与两套模式下的系统底色（原生系统栏取同色用）。 */
function buildTheme(seed: string, variant: SchemeVariant): { css: string; surface: ThemeSurface } {
  const schemes = generateSchemes(seed, variant);
  const tokens = schemeToTokens({ light: schemes.light, dark: schemes.dark });
  const light = withStatus(tokens.light, schemes.light as unknown as Record<string, string>);
  const dark = withStatus(tokens.dark, schemes.dark as unknown as Record<string, string>);
  return {
    css:
      `[data-theme="${THEME_NAME}"]{${block(light)}}` +
      `[data-theme="${THEME_NAME}"][data-theme-mode="dark"]{${block(dark)}}`,
    surface: {
      light: light["--mtrl-sys-color-surface"],
      dark: dark["--mtrl-sys-color-surface"],
    },
  };
}

/** 生成 material 主题 CSS：`[data-theme]` 浅色 + `[data-theme-mode="dark"]` 深色。 */
export function buildThemeCss(seed: string, variant: SchemeVariant): string {
  return buildTheme(seed, variant).css;
}

/**
 * 把主题写进文档。多次调用只替换同一 <style>，避免堆积。
 * 非浏览器环境（如 node 测试）静默跳过。
 *
 * 返回两套模式下的系统底色，调用方据此把原生系统栏调成与 WebView 同色。
 */
export function applyThemeCss(seed: string, variant: SchemeVariant): ThemeSurface {
  const { css, surface } = buildTheme(seed, variant);
  if (typeof document !== "undefined") {
    let style = document.getElementById(THEME_STYLE_ID) as HTMLStyleElement | null;
    if (!style) {
      style = document.createElement("style");
      style.id = THEME_STYLE_ID;
      document.head.append(style);
    }
    style.textContent = css;
  }
  return surface;
}
