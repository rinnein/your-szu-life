import postcss from "postcss";
import presetEnv from "postcss-preset-env";

/** Compile the final stylesheet, including CSS emitted by Tailwind's Vite plugin. */
export function compatibleCss() {
  return {
    name: "szu-compatible-css",
    apply: "build",
    enforce: "post",
    async generateBundle(_options, bundle) {
      for (const asset of Object.values(bundle)) {
        if (asset.type !== "asset" || !asset.fileName.endsWith(".css")) continue;
        const result = await postcss([
          presetEnv({
            browsers: ["Chrome >= 92"],
            stage: 2,
            features: {
              "cascade-layers": true,
              "nesting-rules": true,
              // These need runtime state; preserve native selectors and theme variables.
              "has-pseudo-class": false,
              "custom-properties": false,
            },
          }),
        ]).process(
          typeof asset.source === "string" ? asset.source : new TextDecoder().decode(asset.source),
          { from: asset.fileName, map: false },
        );
        asset.source = result.css;
      }
    },
  };
}
