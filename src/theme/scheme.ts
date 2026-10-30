import {
  Hct,
  SchemeContent,
  SchemeExpressive,
  SchemeFidelity,
  SchemeFruitSalad,
  SchemeMonochrome,
  SchemeNeutral,
  SchemeRainbow,
  SchemeTonalSpot,
  SchemeVibrant,
  argbFromHex,
  hexFromArgb,
  type DynamicScheme,
} from "@material/material-color-utilities";
import { THEME_ROLES, type SchemeRoles } from "material/core/theme";
import type { SchemeVariant } from "./defaults";

type SchemeConstructor = new (seed: Hct, dark: boolean, contrast: number) => DynamicScheme;

const VARIANTS: Record<SchemeVariant, SchemeConstructor> = {
  "tonal-spot": SchemeTonalSpot,
  expressive: SchemeExpressive,
  vibrant: SchemeVibrant,
  neutral: SchemeNeutral,
  fidelity: SchemeFidelity,
  content: SchemeContent,
  monochrome: SchemeMonochrome,
  "fruit-salad": SchemeFruitSalad,
  rainbow: SchemeRainbow,
};

/**
 * material 的 THEME_ROLES 只覆盖 M3 scheme 角色，不含 success/warning/info。
 * 缺失的角色会从基线主题继承，因此这里补齐，保证整套主题自洽。
 */
const STATUS_ROLES = {
  light: {
    success: "#006c4e",
    "on-success": "#ffffff",
    warning: "#974800",
    "on-warning": "#ffffff",
    info: "#0061a4",
    "on-info": "#ffffff",
  },
  dark: {
    success: "#8cd6af",
    "on-success": "#003824",
    warning: "#ffb95c",
    "on-warning": "#462a00",
    info: "#99cbff",
    "on-info": "#003355",
  },
} as const;

/** THEME_ROLES 的 kebab-case 名称 → DynamicScheme 上的 camelCase getter。 */
function getterFor(role: string): keyof DynamicScheme {
  return role.replace(/-(\w)/g, (_, letter: string) => letter.toUpperCase()) as keyof DynamicScheme;
}

function readRoles(scheme: DynamicScheme, extra: Record<string, string>): SchemeRoles {
  const roles: Record<string, string> = { ...extra };
  for (const role of THEME_ROLES) {
    const value = scheme[getterFor(role)];
    // shadow / scrim 是常量 0xff000000，其余角色都是 ARGB 数值。
    roles[role] = typeof value === "number" ? hexFromArgb(value) : "#000000";
  }
  return roles as SchemeRoles;
}

export interface GeneratedSchemes {
  light: SchemeRoles;
  dark: SchemeRoles;
}

/**
 * 分类色板（accent）数量。与 primary/secondary/tertiary/error 等语义角色无关，
 * 专门供「同一页面需要大量互相区分的颜色」的场景使用（如月历待办分类点）。
 */
export const ACCENT_COUNT = 12;

/**
 * 分类色的固定 HCT 彩度与明度。彩度取 M3 主色同级的 48；明度对齐 M3
 * 浅色 tone 40~50 / 深色 tone 80 的习惯，保证在各自模式的表面上可读。
 * 色相从种子色相偏移半个步长开始均分整圈，因此没有任何一档落在种子
 * 色相（primary）上；即使 scheme 变体是 monochrome，分类标记也保持
 * 有彩度——它们必须彼此可区分，不承担品牌语义。
 */
function accentColor(baseHue: number, index: number, dark: boolean): string {
  const hue = (baseHue + 360 / ACCENT_COUNT / 2 + (index * 360) / ACCENT_COUNT) % 360;
  const hct = Hct.from(hue, 48, dark ? 80 : 45);
  return hexFromArgb(hct.toInt());
}

/** 从种子色生成当前模式的分类色板（accent-01…accent-12 的十六进制值）。 */
export function generateAccents(seed: string, dark: boolean): string[] {
  const baseHue = Hct.fromInt(argbFromHex(seed)).hue;
  return Array.from({ length: ACCENT_COUNT }, (_, index) => accentColor(baseHue, index, dark));
}

/** 从种子色生成单一模式的一套角色表；预览等只需要一侧的调用方使用。 */
export function generateScheme(
  seed: string,
  variant: SchemeVariant,
  dark: boolean,
  contrast = 0,
): SchemeRoles {
  const Scheme = VARIANTS[variant] ?? SchemeTonalSpot;
  const hct = Hct.fromInt(argbFromHex(seed));
  return readRoles(new Scheme(hct, dark, contrast), dark ? STATUS_ROLES.dark : STATUS_ROLES.light);
}

/** 从种子色生成成对的 light/dark 角色表。种子非法时抛错，调用方需先归一化。 */
export function generateSchemes(
  seed: string,
  variant: SchemeVariant,
  contrast = 0,
): GeneratedSchemes {
  const Scheme = VARIANTS[variant] ?? SchemeTonalSpot;
  const hct = Hct.fromInt(argbFromHex(seed));
  return {
    light: readRoles(new Scheme(hct, false, contrast), STATUS_ROLES.light),
    dark: readRoles(new Scheme(hct, true, contrast), STATUS_ROLES.dark),
  };
}
