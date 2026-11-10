// @vitest-environment node
import { describe, expect, it } from "vite-plus/test";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { createContext, runInContext } from "node:vm";
import { parse } from "rolldown/utils";
import { buildCompatibilityRuntime, transformMonacoModule } from "./modern-monaco-assets.mjs";

describe("copied Monaco modules and isolated Worker realms", () => {
  it("bundles nested helpers without reintroducing class static blocks", async () => {
    const directory = mkdtempSync(join(tmpdir(), "szu-monaco-compat-"));
    try {
      const filename = join(directory, "module.mjs");
      const source =
        "export class Example { #value; static { this.ready = true; } has(value) { return #value in value; } }";
      writeFileSync(filename, source);
      const code = await transformMonacoModule(filename, source);
      const ast = (await parse(filename, code)).program;
      const imports = ast.body.filter((node) => node.type === "ImportDeclaration");
      expect(imports).toHaveLength(0);
      const containsStaticBlock = (node) =>
        node &&
        typeof node === "object" &&
        (node.type === "StaticBlock" ||
          Object.values(node).some((value) =>
            Array.isArray(value) ? value.some(containsStaticBlock) : containsStaticBlock(value),
          ));
      expect(containsStaticBlock(ast)).toBe(false);
      writeFileSync(filename, code);
      const { Example } = await import(/* @vite-ignore */ pathToFileURL(filename).href);
      expect(Example.ready).toBe(true);
      const instance = new Example();
      expect(instance.has(instance)).toBe(true);
      expect(instance.has({})).toBe(false);
    } finally {
      rmSync(directory, { recursive: true });
    }
  });
  it("installs idempotently in the Worker without patching the page realm", async () => {
    const code = await buildCompatibilityRuntime();
    const page = createContext({});
    const worker = createContext({});
    for (const realm of [page, worker])
      runInContext("delete Object.hasOwn; delete Array.prototype.at", realm);
    runInContext(code, worker);
    expect(runInContext("Object.hasOwn({own: 1}, 'own') && [1,2].at(-1) === 2", worker)).toBe(true);
    expect(runInContext("typeof Object.hasOwn", page)).toBe("undefined");
    runInContext("globalThis.installed = Object.hasOwn", worker);
    runInContext(code, worker);
    expect(runInContext("installed === Object.hasOwn", worker)).toBe(true);
  });
});
