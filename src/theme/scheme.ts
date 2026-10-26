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
