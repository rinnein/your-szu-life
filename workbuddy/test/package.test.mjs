import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { validatePackage } from "../lib/validate.mjs";
import { createZip, listZipEntries } from "../lib/zip.mjs";
import { buildExpertZip } from "../scripts/build.mjs";

const pluginRoot = fileURLToPath(new URL("../plugins/your-szu-life", import.meta.url));

test("expert package passes the open-platform validation", async () => {
  const result = await validatePackage(pluginRoot);
  assert.deepEqual(result.errors, []);
  assert.equal(result.ok, true);
  assert.equal(result.manifest.name, "your-szu-life");
  assert.equal(result.manifest.version, "1.0.0");
  const description = result.manifest.displayDescription.zh;
  assert.ok([...description].length >= 40 && [...description].length <= 50);
});

test("store-only zip round-trips names and modes", () => {
  const zip = createZip([
    { name: "your-szu-life/a.txt", data: Buffer.from("hello"), mode: 0o644 },
    { name: "your-szu-life/bin/tool", data: Buffer.from("#!/bin/sh\n"), mode: 0o755 },
  ]);
  const entries = listZipEntries(zip);
  assert.deepEqual(
    entries.map((entry) => entry.name),
    ["your-szu-life/a.txt", "your-szu-life/bin/tool"],
  );
  assert.equal(entries[1].mode, 0o755);
});

test("buildExpertZip produces a top-level package with hidden files", async () => {
  const dist = await mkdtemp(path.join(os.tmpdir(), "szu-life-dist-"));
  const result = await buildExpertZip({ pluginRoot, distRoot: dist });
  assert.equal(result.ok, true);
  const zip = await readFile(result.output);
  const entries = listZipEntries(zip);
  const names = entries.map((entry) => entry.name);
  assert.ok(names.includes("your-szu-life/.codebuddy-plugin/plugin.json"));
  assert.ok(names.includes("your-szu-life/.mcp.json"));
  assert.ok(names.includes("your-szu-life/bin/szu-life.mjs"));
  const launcher = entries.find((entry) => entry.name === "your-szu-life/bin/szu-life");
  assert.equal(launcher.mode, 0o755);
  assert.ok(!names.some((name) => name.includes("dist/")));
});
