import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";

import { installAppImage } from "../plugins/your-szu-life/bin/lib/install-linux.mjs";
import {
  desktopFilePath,
  appImagePath,
  launcherPath,
} from "../plugins/your-szu-life/bin/lib/paths.mjs";
import { createTempDataHome } from "./helpers.mjs";
import { sha256File } from "../plugins/your-szu-life/bin/lib/util.mjs";

test("other distributions install an AppImage into XDG directories", async () => {
  const dataHome = await createTempDataHome();
  const binHome = path.join(dataHome, "bin");
  process.env.XDG_BIN_HOME = binHome;
  await mkdir(binHome, { recursive: true });

  // A 6 MiB fake ELF is enough for the install path; it is never executed.
  const source = path.join(dataHome, "fake-appimage");
  const payload = Buffer.alloc(6 * 1024 * 1024);
  Buffer.from([0x7f, 0x45, 0x4c, 0x46]).copy(payload, 0);
  await writeFile(source, payload);
  const digest = await sha256File(source);

  const plan = {
    version: "0.19.0",
    channel: "stable",
    fallback: false,
    source: "test",
    asset: {
      name: "your-szu-life_linux_x64_0.19.0-release.AppImage",
      sha256: digest,
    },
    candidates: [`file://${source}`],
  };

  const result = await installAppImage(plan, { yes: true });
  assert.equal(result.ok, true);
  assert.equal(result.version, "0.19.0");
  assert.equal(await stat(appImagePath()).then((info) => info.size), payload.length);

  const launcher = await readFile(launcherPath(), "utf8");
  assert.match(launcher, /managed by the your-szu-life WorkBuddy expert/);
  assert.match(launcher, /exec "/);

  const desktop = await readFile(desktopFilePath(), "utf8");
  assert.match(desktop, /Exec=".*" %u/);
  assert.match(desktop, /MimeType=x-scheme-handler\/your-szu-life;/);

  const installMeta = JSON.parse(
    await readFile(path.join(dataHome, "in.rinne.your-szu-life", "install.json"), "utf8"),
  );
  assert.equal(installMeta.version, "0.19.0");
  assert.equal(installMeta.format, "appimage");
});
