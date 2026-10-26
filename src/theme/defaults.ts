/** 主题预设。种子色决定整套 M3 角色，见 scheme.ts。 */

export type ThemeMode = "light" | "dark" | "system";

/**
 * M3 scheme 变体。tonal-spot 是 Android DynamicColors 的默认策略，也是唯一
 * 保证 WebView 与原生标题栏配色一致的选项；其余为可选风格。
 */
export type SchemeVariant =
  | "tonal-spot"
  | "expressive"
  | "vibrant"
  | "neutral"
  | "fidelity"
  | "content"
  | "monochrome"
  | "fruit-salad"
  | "rainbow";

export type SeedSource = "wallpaper" | "user" | "default";

export interface Appearance {
  mode: ThemeMode;
  seed: string;
  variant: SchemeVariant;
  seedSource: SeedSource;
}

/** 品牌默认种子，沿用迁移前的 --szu-brand。 */
export const DEFAULT_SEED = "#5d5fef";

export const DEFAULT_APPEARANCE: Appearance = {
  mode: "system",
  seed: DEFAULT_SEED,
  variant: "tonal-spot",
  seedSource: "default",
};

export interface SeedPreset {
  id: string;
  label: string;
  seed: string;
}

export const SEED_PRESETS: readonly SeedPreset[] = [
  { id: "indigo", label: "靛蓝", seed: DEFAULT_SEED },
  { id: "ocean", label: "海洋", seed: "#00639b" },
  { id: "forest", label: "森林", seed: "#2e6c3e" },
  { id: "sunset", label: "日落", seed: "#b3261e" },
  { id: "amber", label: "琥珀", seed: "#8a5100" },
  { id: "plum", label: "梅子", seed: "#7d5260" },
] as const;

export const VARIANT_LABELS: Readonly<Record<SchemeVariant, string>> = {
  "tonal-spot": "跟随壁纸（Tonal Spot）",
  expressive: "表现力（Expressive）",
  vibrant: "鲜艳（Vibrant）",
  neutral: "中性（Neutral）",
  fidelity: "还原种子（Fidelity）",
  content: "内容优先（Content）",
  monochrome: "单色（Monochrome）",
  "fruit-salad": "果味（Fruit Salad）",
  rainbow: "彩虹（Rainbow）",
};

export const MODE_LABELS: Readonly<Record<ThemeMode, string>> = {
  light: "浅色",
  dark: "深色",
  system: "跟随系统",
};

/** 归一化任意输入为 #rrggbb 小写；非法输入返回 undefined。 */
export function normalizeSeed(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  const withHash = trimmed.startsWith("#") ? trimmed : `#${trimmed}`;
  return /^#[0-9a-f]{6}$/i.test(withHash) ? withHash.toLowerCase() : undefined;
}
