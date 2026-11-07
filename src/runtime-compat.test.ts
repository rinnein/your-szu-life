import { afterEach, describe, expect, it, vi } from "vite-plus/test";

const original = Object.getOwnPropertyDescriptor(Object, "hasOwn")!;
afterEach(() => {
  Object.defineProperty(Object, "hasOwn", original);
  vi.resetModules();
});

describe("Object.hasOwn on legacy WebView", () => {
  it("keeps a supported native implementation", async () => {
    const native = Object.hasOwn;
    await import("./runtime-compat");
    expect(Object.hasOwn).toBe(native);
  });

  it("handles own keys, primitives, symbols and sparse arrays without reading values", async () => {
    Object.defineProperty(Object, "hasOwn", {
      configurable: true,
      writable: true,
      value: undefined,
    });
    await import("./runtime-compat");
    const symbol = Symbol("own");
    const object = Object.assign(Object.create({ inherited: 1 }), {
      [symbol]: 1,
      own: undefined,
      hasOwnProperty: null,
    });
    Object.defineProperty(object, "getter", {
      get: () => {
        throw Error("must not read values");
      },
    });
    expect(Object.hasOwn(object, "own")).toBe(true);
    expect(Object.hasOwn(object, "inherited")).toBe(false);
    expect(Object.hasOwn(object, symbol)).toBe(true);
    expect(Object.hasOwn(object, "getter")).toBe(true);
    expect(Object.hasOwn("abc", 1)).toBe(true);
    expect(Object.hasOwn(42, "toString")).toBe(false);
    const sparse: undefined[] = [];
    sparse.length = 2;
    sparse[1] = undefined;
    expect(Object.hasOwn(sparse, 0)).toBe(false);
    expect(Object.hasOwn(sparse, 1)).toBe(true);
    expect(() => Object.hasOwn(null, "key")).toThrow(TypeError);
    expect(() => Object.hasOwn(undefined, "key")).toThrow(TypeError);
    expect(Object.keys(Object)).not.toContain("hasOwn");
  });
});
