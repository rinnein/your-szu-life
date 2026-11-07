// Vite lowers syntax but does not supply runtime APIs used by routed dependencies.
// Load this before React/router so es-toolkit and Recharts work on Chrome 92.
declare global {
  interface ObjectConstructor {
    hasOwn(object: unknown, property: PropertyKey): boolean;
  }
}

if (typeof Object.hasOwn !== "function") {
  Object.defineProperty(Object, "hasOwn", {
    configurable: true,
    writable: true,
    value: function hasOwn(object: unknown, property: PropertyKey): boolean {
      return Object.prototype.hasOwnProperty.call(object, property);
    },
  });
}

export {};
