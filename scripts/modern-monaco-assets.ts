import { resolve } from "node:path";
import { rolldown } from "rolldown";
import { transform } from "rolldown/utils";
import { viteStaticCopy } from "vite-plugin-static-copy";
import { WEBVIEW_TARGET } from "../src/compat/contracts";

export async function buildCompatibilityRuntime() {
  const bundle = await rolldown({
    input: resolve("src/compat/ecmascript.ts"),
    platform: "browser",
    transform: { target: WEBVIEW_TARGET },
  });
  try {
    const { output } = await bundle.generate({ format: "iife", minify: true });
    return output[0].code;
  } finally {
    await bundle.close();
  }
}

export async function transformMonacoModule(filename: string, code: string) {
  const result = await transform(filename, code, { target: WEBVIEW_TARGET, sourceType: "module" });
  if (result.errors.length)
    throw new Error(
      `Monaco compatibility transform failed: ${filename}: ${result.errors.map((e) => e.message).join(", ")}`,
    );
  let convertedCode = result.code;
  if (convertedCode.includes("@oxc-project/runtime")) {
    // 保留文件间的相对 URL，只打包 Oxc helper（包括 Rolldown 的虚拟 helper）。
    const helpers = await rolldown({
      input: filename,
      platform: "browser",
      transform: { target: WEBVIEW_TARGET },
      external: (id, importer) => {
        const runtimePath = (path: string) => /@oxc-project(?:\/runtime|\+runtime@)/.test(path);
        return !!importer && !runtimePath(id) && !runtimePath(importer);
      },
      plugins: [
        {
          name: "monaco-transformed-module",
          load: (id) => (id === filename ? convertedCode : undefined),
        },
      ],
    });
    try {
      const { output } = await helpers.generate({ format: "es", minify: true });
      convertedCode = output[0].code;
    } finally {
      await helpers.close();
    }
  }
  return convertedCode;
}

// Serve/copy npm resources at the same URLs in development and native bundles.
// The upstream TS worker imports a bare compiler module; bundle that worker for browsers.
export function modernMonacoAssets() {
  let typescriptWorker;
  let runtime;
  const converted = new Map();
  const workerRuntime = () => (runtime ??= buildCompatibilityRuntime());
  return viteStaticCopy({
    targets: [
      {
        src: "node_modules/modern-monaco/dist/**/*.mjs",
        dest: "vendor/modern-monaco",
        rename: { stripBase: 3 },
        transform: async (content, filename) => {
          if (converted.has(filename)) return converted.get(filename);
          const normalized = filename.replaceAll("\\", "/");
          const convertModule = async (code: string) => {
            // 每个被复制的模块都先执行幂等补丁，确保 Worker 的静态依赖也受覆盖。
            return (await workerRuntime()) + "\n" + (await transformMonacoModule(filename, code));
          };
          if (!normalized.endsWith("/lsp/typescript/worker.mjs")) {
            const result = await convertModule(content.toString());
            converted.set(filename, result);
            return result;
          }
          typescriptWorker ??= (async () => {
            const bundle = await rolldown({
              input: filename,
              platform: "browser",
              transform: { target: WEBVIEW_TARGET },
              resolve: {
                alias: { typescript: resolve("node_modules/typescript-lsp/lib/typescript.js") },
              },
            });
            try {
              const { output } = await bundle.generate({ format: "es", minify: true });
              return await convertModule(output[0].code);
            } finally {
              await bundle.close();
            }
          })();
          return typescriptWorker;
        },
      },
      {
        src: "node_modules/tm-grammars/grammars/{c,cpp,cpp-macro,regexp,glsl,sql,java,python,go}.json",
        dest: "tm-grammars@1.31.15/grammars",
        rename: { stripBase: true },
      },
      {
        src: "node_modules/tm-themes/themes/vitesse-{light,dark}.json",
        dest: "tm-themes@1.12.2/themes",
        rename: { stripBase: true },
      },
    ],
  });
}
