import test from "node:test";
import assert from "node:assert/strict";

import {
  detectLinuxFamily,
  detectPlatform,
  parseOsRelease,
} from "../plugins/your-szu-life/bin/lib/platform.mjs";

test("parseOsRelease handles quotes and comments", () => {
  const fields = parseOsRelease(`
# comment
ID=ubuntu
ID_LIKE="debian ubuntu"
PRETTY_NAME="Ubuntu 26.04 LTS"
`);
  assert.equal(fields.ID, "ubuntu");
  assert.equal(fields.ID_LIKE, "debian ubuntu");
  assert.equal(fields.PRETTY_NAME, "Ubuntu 26.04 LTS");
});

test("detectLinuxFamily recognises Debian and Arch families", () => {
  assert.equal(detectLinuxFamily("ID=ubuntu\nID_LIKE=debian\n"), "debian");
  assert.equal(detectLinuxFamily('ID=linuxmint\nID_LIKE="ubuntu debian"\n'), "debian");
  assert.equal(detectLinuxFamily("ID=manjaro\nID_LIKE=arch\n"), "arch");
  assert.equal(detectLinuxFamily("ID=cachyos\nID_LIKE=arch\n"), "arch");
});

test("detectLinuxFamily sends Fedora/NixOS/SteamOS to AppImage", () => {
  assert.equal(detectLinuxFamily("ID=fedora\n"), "other");
  assert.equal(detectLinuxFamily("ID=nixos\n"), "other");
  assert.equal(detectLinuxFamily("ID=steamos\nID_LIKE=arch\n"), "other");
  assert.equal(detectLinuxFamily("ID=bazzite\nID_LIKE=arch\n"), "other");
});

test("detectLinuxFamily falls back to package managers", () => {
  assert.equal(detectLinuxFamily("ID=unknown\n", { hasApt: true }), "debian");
  assert.equal(detectLinuxFamily("ID=unknown\n", { hasPacman: true }), "arch");
  assert.equal(detectLinuxFamily("ID=unknown\n"), "other");
});

test("detectPlatform returns a consistent shape", () => {
  const platform = detectPlatform();
  assert.equal(typeof platform.os, "string");
  assert.equal(typeof platform.arch, "string");
  assert.equal(typeof platform.desktop, "boolean");
  assert.equal(typeof platform.supported, "boolean");
  if (!platform.supported) {
    assert.ok(platform.reason.length > 0);
  }
});
