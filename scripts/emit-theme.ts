/**
 * 重新生成 src/theme/default-theme.css。
 *
 * 默认主题以静态 CSS 形式随包发布，首帧即为正确配色；运行时换色由
 * src/theme/tokens.ts 覆盖。改动 DEFAULT_SEED 或默认变体后运行：
 *
 *   vpx scripts/emit-theme.ts
 */
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { DEFAULT_SEED } from "../src/theme/defaults.ts";
import { buildThemeCss } from "../src/theme/tokens.ts";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const variant = "tonal-spot";
const css = buildThemeCss(DEFAULT_SEED, variant);
const darkAt = css.indexOf('[data-theme="szu"][data-theme-mode="dark"]');
const light = css.slice(0, darkAt).trim();
const dark = css.slice(darkAt).trim();

const header = `/*
 * 默认主题（种子 ${DEFAULT_SEED}，Tonal Spot）。由 \`vpx scripts/emit-theme.ts\` 生成，
 * 目的是让首帧就有正确配色，避免等待 JS 生成时闪一下基线紫色。
 * 运行时换色会覆盖同 id 的 <style>，见 src/theme/tokens.ts。
 */
`;

writeFileSync(join(root, "src/theme/default-theme.css"), `${header}${light}\n${dark}\n`);
console.log(`已写入 src/theme/default-theme.css（种子 ${DEFAULT_SEED}）`);
