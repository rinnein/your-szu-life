// 每个 JS realm 都必须执行：页面的补丁不会传给 Worker。
import "core-js/actual/object/has-own";
import "core-js/actual/array/at";
import "core-js/actual/string/at";
import "core-js/actual/typed-array/at";

/** 保留原生实现；随机数始终来自系统 CSPRNG。 */
export function installRandomUUID(crypto: Crypto): void {
  if (typeof crypto.randomUUID === "function") return;
  Object.defineProperty(crypto, "randomUUID", {
    configurable: true,
    value: () => {
      const bytes = crypto.getRandomValues(new Uint8Array(16));
      bytes[6] = (bytes[6] & 15) | 64;
      bytes[8] = (bytes[8] & 63) | 128;
      const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
      return `${hex.slice(0, 4).join("")}-${hex.slice(4, 6).join("")}-${hex.slice(6, 8).join("")}-${hex.slice(8, 10).join("")}-${hex.slice(10).join("")}`;
    },
  });
}
if (typeof globalThis.crypto !== "undefined") installRandomUUID(globalThis.crypto);
