import { describe, expect, it } from "vite-plus/test";
import { argbFromHex, Hct } from "@material/material-color-utilities";
import { DEFAULT_SEED, normalizeSeed, SEED_PRESETS } from "./defaults";
import { ACCENT_COUNT, generateAccents, generateScheme, generateSchemes } from "./scheme";
import { buildThemeCss, THEME_NAME } from "./tokens";

describe("normalizeSeed", () => {
  it("accepts #rrggbb", () => {
    expect(normalizeSeed("#5d5fef")).toBe("#5d5fef");
    expect(normalizeSeed("#5D5FEF")).toBe("#5d5fef");
  });

  it("adds a missing hash", () => {
    expect(normalizeSeed("5d5fef")).toBe("#5d5fef");
  });

  it("trims whitespace", () => {
    expect(normalizeSeed("  #5d5fef  ")).toBe("#5d5fef");
  });

  it("rejects malformed input", () => {
    for (const value of ["", "#", "#fff", "#gggggg", "red", "#5d5fef0", null, undefined, 42]) {
      expect(normalizeSeed(value)).toBeUndefined();
    }
  });
});

describe("seed presets", () => {
  it("are all valid hex colours", () => {
    for (const preset of SEED_PRESETS) {
      expect(normalizeSeed(preset.seed)).toBe(preset.seed);
    }
  });

  it("include the default seed", () => {
    expect(SEED_PRESETS.some((preset) => preset.seed === DEFAULT_SEED)).toBe(true);
  });
});

describe("generateSchemes", () => {
  it("produces every M3 role plus status colours", () => {
    const { light, dark } = generateSchemes(DEFAULT_SEED, "tonal-spot");
    for (const scheme of [light, dark]) {
      expect(scheme.primary).toMatch(/^#[0-9a-f]{6}$/);
      expect(scheme.surface).toMatch(/^#[0-9a-f]{6}$/);
      expect(scheme.success).toMatch(/^#[0-9a-f]{6}$/);
      expect(scheme.warning).toMatch(/^#[0-9a-f]{6}$/);
      expect(scheme.info).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("generates a single mode identical to the paired helper", () => {
    const pair = generateSchemes(DEFAULT_SEED, "tonal-spot");
    expect(generateScheme(DEFAULT_SEED, "tonal-spot", false)).toEqual(pair.light);
    expect(generateScheme(DEFAULT_SEED, "tonal-spot", true)).toEqual(pair.dark);
  });

  it("keeps the seed hue for tonal-spot", () => {
    // tonal-spot 保留种子色相，因此主色应落在蓝紫区间而不是被旋转到别的色系。
    const { light } = generateSchemes(DEFAULT_SEED, "tonal-spot");
    const red = parseInt(light.primary.slice(1, 3), 16);
    const blue = parseInt(light.primary.slice(5, 7), 16);
    expect(blue).toBeGreaterThan(red);
  });

  it("gives light and dark different surfaces", () => {
    const { light, dark } = generateSchemes(DEFAULT_SEED, "tonal-spot");
    expect(light.surface).not.toBe(dark.surface);
  });

  it("is deterministic", () => {
    const first = generateSchemes(DEFAULT_SEED, "tonal-spot");
    const second = generateSchemes(DEFAULT_SEED, "tonal-spot");
    expect(first).toEqual(second);
  });

  it("changes colours when the seed changes", () => {
    const a = generateSchemes("#5d5fef", "tonal-spot");
    const b = generateSchemes("#b3261e", "tonal-spot");
    expect(a.light.primary).not.toBe(b.light.primary);
  });
});

describe("generateAccents", () => {
  it("produces the full palette of distinct hex colours", () => {
    const accents = generateAccents(DEFAULT_SEED, false);
    expect(accents).toHaveLength(ACCENT_COUNT);
    expect(new Set(accents).size).toBe(ACCENT_COUNT);
    for (const accent of accents) {
      expect(accent).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("adapts tones between light and dark modes", () => {
    const light = generateAccents(DEFAULT_SEED, false);
    const dark = generateAccents(DEFAULT_SEED, true);
    // 深色模式的分类色应是浅淡的高明度色调（tone 80），明度分量更大。
    for (let index = 0; index < ACCENT_COUNT; index += 1) {
      const channelSum = (hex: string) =>
        hex
          .slice(1)
          .match(/.{2}/g)!
          .reduce((sum, channel) => sum + parseInt(channel, 16), 0);
      expect(channelSum(dark[index])).toBeGreaterThan(channelSum(light[index]));
    }
  });

  it("offsets half a step away from the seed hue", () => {
    // 整圈均分且偏移半个步长：没有任何一档落在种子色相（primary）附近。
    const seedHue = Hct.fromInt(argbFromHex(DEFAULT_SEED)).hue;
    for (const accent of generateAccents(DEFAULT_SEED, false)) {
      const hue = Hct.fromInt(argbFromHex(accent)).hue;
      const distance = Math.abs(((((hue - seedHue) % 360) + 540) % 360) - 180);
      expect(distance).toBeGreaterThan(5);
    }
  });
});

describe("buildThemeCss", () => {
  it("emits scoped light and dark blocks", () => {
    const css = buildThemeCss(DEFAULT_SEED, "tonal-spot");
    expect(css).toContain(`[data-theme="${THEME_NAME}"]{`);
    expect(css).toContain(`[data-theme="${THEME_NAME}"][data-theme-mode="dark"]{`);
    expect(css).toContain("--mtrl-sys-color-primary:");
    expect(css).toContain("--mtrl-sys-color-success:");
  });

  it("emits the categorical accent palette in both modes", () => {
    const css = buildThemeCss(DEFAULT_SEED, "tonal-spot");
    for (let index = 1; index <= ACCENT_COUNT; index += 1) {
      const name = `--mtrl-sys-color-accent-${String(index).padStart(2, "0")}`;
      expect(css).toContain(`${name}:`);
    }
  });

  it("does not leak an unscoped :root block", () => {
    // 未加作用域的 :root 会污染 material 的基线主题，也会让嵌套主题失效。
    expect(buildThemeCss(DEFAULT_SEED, "tonal-spot")).not.toContain(":root{");
  });
});
