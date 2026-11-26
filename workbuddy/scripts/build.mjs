#!/usr/bin/env node
// Build the WorkBuddy expert ZIP after validating the package contract.

import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { validatePackage } from "../lib/validate.mjs";
import { createZip, walkFiles } from "../lib/zip.mjs";

const here = fileURLToPath(import.meta.url);
const workbuddyRoot = path.resolve(path.dirname(here), "..");
const pluginRoot = path.join(workbuddyRoot, "plugins", "your-szu-life");
const distRoot = path.join(workbuddyRoot, "dist");

const WHITELIST_DIRS = [".codebuddy-plugin", "agents", "skills", "bin", "avatars"];
const WHITELIST_FILES = [".mcp.json", "README.md"];

export async function buildExpertZip(options = {}) {
  const root = options.pluginRoot ?? pluginRoot;
  const dist = options.distRoot ?? distRoot;

  const validation = await validatePackage(root);
  if (!validation.ok) {
    return { ok: false, errors: validation.errors, warnings: validation.warnings };
  }

  const files = [];
  for (const directory of WHITELIST_DIRS) {
    const absolute = path.join(root, directory);
    try {
      const info = await stat(absolute);
      if (!info.isDirectory()) continue;
    } catch {
      continue;
    }
    files.push(...(await walkFiles(root, directory)));
  }
  for (const file of WHITELIST_FILES) {
    try {
      const info = await stat(path.join(root, file));
      if (info.isFile()) files.push(file);
    } catch {
      // validatePackage already reported missing required files
    }
  }

  files.sort();
  const version = validation.manifest.version;
  const prefix = "your-szu-life";
  const entries = [];
  for (const relative of files) {
    const absolute = path.join(root, relative);
    const info = await stat(absolute);
    const data = await readFile(absolute);
    const mode = relative === "bin/szu-life" || relative === "bin/szu-life.mjs" ? 0o755 : 0o644;
    entries.push({
      name: `${prefix}/${relative.replace(/\\/g, "/")}`,
      data,
      mode: info.isFile() ? mode : 0o755,
    });
  }

  const zip = createZip(entries);
  await mkdir(dist, { recursive: true });
  const output = path.join(dist, `your-szu-life-workbuddy-${version}.zip`);
  await writeFile(output, zip, { mode: 0o644 });

  return {
    ok: true,
    output,
    bytes: zip.length,
    files: entries.map((entry) => entry.name),
    version,
    warnings: validation.warnings,
  };
}

async function main() {
  const result = await buildExpertZip();
  if (!result.ok) {
    console.error("WorkBuddy 专家包校验失败：");
    for (const error of result.errors) console.error(`- ${error}`);
    process.exitCode = 1;
    return;
  }
  console.log(`已生成 ${result.output}`);
  console.log(`版本 ${result.version}，${result.files.length} 个文件，${result.bytes} bytes`);
  for (const warning of result.warnings ?? []) console.warn(`警告: ${warning}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main();
}
