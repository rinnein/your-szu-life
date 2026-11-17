import { describe, expect, it } from "vite-plus/test";
import { navigationIndicatorBox } from "./navigation-indicator";

const origin = { left: 0, top: 779, width: 390, height: 65 };

describe("navigationIndicatorBox", () => {
  it("measures the target relative to the shell padding box", () => {
    expect(
      navigationIndicatorBox({ left: 30, top: 785, width: 56, height: 32 }, origin, {
        borderTop: 1,
        borderLeft: 0,
      }),
    ).toEqual({ left: 30, top: 5, width: 56, height: 32 });
  });

  it("subtracts the start border from the offset", () => {
    expect(
      navigationIndicatorBox({ left: 30, top: 785, width: 56, height: 32 }, origin, {
        borderTop: 1,
        borderLeft: 4,
      }),
    ).toEqual({ left: 26, top: 5, width: 56, height: 32 });
  });

  it("returns null when the indicator has no size yet", () => {
    expect(navigationIndicatorBox({ left: 0, top: 0, width: 0, height: 0 }, origin)).toBeNull();
  });
});
