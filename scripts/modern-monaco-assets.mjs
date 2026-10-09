import { resolve } from "node:path";
import { rolldown } from "rolldown";
import { viteStaticCopy } from "vite-plugin-static-copy";

// Serve/copy npm resources at the same URLs in development and native bundles.
// The upstream TS worker imports a bare compiler module; bundle that worker for browsers.
export function modernMonacoAssets() {
  let typescriptWorker;
  return viteStaticCopy({
    targets: [
      {
        src: "node_modules/modern-monaco/dist/**/*.mjs",
        dest: "vendor/modern-monaco",
        rename: { stripBase: 3 },
        transform: async (content, filename) => {
          if (!filename.replaceAll("\\", "/").endsWith("/lsp/typescript/worker.mjs"))
            return content;
          typescriptWorker ??= (async () => {
            const bundle = await rolldown({
              input: filename,
              platform: "browser",
              resolve: {
                alias: { typescript: resolve("node_modules/typescript-lsp/lib/typescript.js") },
              },
            });
            try {
              const { output } = await bundle.generate({ format: "es", minify: true });
              return output[0].code;
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
