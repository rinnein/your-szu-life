import { describe, expect, it } from "vite-plus/test";
import { compatibilityThemeTokens, evaluateColorMix } from "./colors";
import { mixProperty } from "./contracts";
import { generateSchemes } from "../theme/scheme";

describe("Chromium 90 dynamic colors", () => {
  it("premultiplies transparent colors and supports rgb commas", () => {
    expect(evaluateColorMix("color-mix(in srgb, rgb(255, 0, 0) 20%, transparent)")).toBe(
      "rgba(255, 0, 0, 0.2)",
    );
    expect(evaluateColorMix("color-mix(in srgb, red 20%, blue)")).toBe("rgb(51, 0, 204)");
    expect(
      evaluateColorMix("color-mix(in srgb, currentColor 8%, transparent)", "rgb(0, 128, 0)"),
    ).toBe("rgba(0, 128, 0, 0.08)");
    expect(evaluateColorMix("color-mix(in srgb, red 0%, blue 0%)")).toBeUndefined();
  });
  it("retains the requested oklab color space", () => {
    expect(evaluateColorMix("color-mix(in oklab, red 50%, blue)")).not.toBe(
      evaluateColorMix("color-mix(in srgb, red 50%, blue)"),
    );
  });
  it("regenerates fallback colors for both modes and a changed seed", () => {
    const schemes = generateSchemes("#ff0000", "tonal-spot");
    const other = generateSchemes("#0000ff", "tonal-spot");
    const tokens = (roles: Record<string, string>) =>
      compatibilityThemeTokens(
        Object.fromEntries(
          Object.entries(roles).map(([key, value]) => [`--mtrl-sys-color-${key}`, value]),
        ),
      );
    const light = tokens(schemes.light as unknown as Record<string, string>);
    const dark = tokens(schemes.dark as unknown as Record<string, string>);
    const blue = tokens(other.light as unknown as Record<string, string>);
    expect(light["--szu-compat-rgb-primary"]).not.toBe(dark["--szu-compat-rgb-primary"]);
    expect(light["--szu-compat-rgb-primary"]).not.toBe(blue["--szu-compat-rgb-primary"]);
    const mixed = mixProperty(
      "color-mix(in srgb, var(--mtrl-sys-color-on-surface) 8%, var(--mtrl-sys-color-surface-container-high))",
    );
    expect(light[mixed]).toMatch(/^rgb/);
    expect(light[mixed]).not.toBe(dark[mixed]);
  });
});
