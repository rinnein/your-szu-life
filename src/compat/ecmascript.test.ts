import { describe, expect, it } from "vite-plus/test";
import { installRandomUUID } from "./ecmascript";

describe("runtime compatibility", () => {
  it("provides Object.hasOwn without inheriting or trusting shadowed properties", () => {
    const hasOwn = (Object as unknown as { hasOwn: (object: object, key: PropertyKey) => boolean })
      .hasOwn;
    const object = Object.assign(Object.create({ inherited: true }), {
      own: true,
      hasOwnProperty: false,
    });
    expect(hasOwn(object, "own")).toBe(true);
    expect(hasOwn(object, "inherited")).toBe(false);
    expect(hasOwn(Object.create(null), "own")).toBe(false);
    expect(([1, 2] as number[] & { at: (index: number) => number }).at(-1)).toBe(2);
  });
  it("uses getRandomValues for RFC4122 UUIDs and installs once", () => {
    const crypto = { getRandomValues: (array: Uint8Array) => array.fill(255) } as unknown as Crypto;
    installRandomUUID(crypto);
    const first = crypto.randomUUID;
    expect(first()).toBe("ffffffff-ffff-4fff-bfff-ffffffffffff");
    installRandomUUID(crypto);
    expect(crypto.randomUUID).toBe(first);
  });
});
