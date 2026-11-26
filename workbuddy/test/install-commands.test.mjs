import test from "node:test";
import assert from "node:assert/strict";

import {
  archInstallScript,
  debianInstallScript,
  installCommandFor,
} from "../plugins/your-szu-life/bin/lib/install-linux.mjs";

const plan = {
  version: "0.18.0",
  channel: "stable",
  fallback: false,
  asset: { sha256: "a".repeat(64) },
  candidates: [
    "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.18.0/your-szu-life_linux_x64_0.18.0-release.deb",
    "https://gh-proxy.com/https://github.com/rinnein/your-szu-life/releases/download/v0.18.0/your-szu-life_linux_x64_0.18.0-release.deb",
  ],
};

test("Debian script downloads a deb and installs via apt", () => {
  const script = debianInstallScript(plan);
  assert.match(script, /mktemp -d/);
  assert.match(script, /trap 'rm -rf "\$tmp_dir"' EXIT/);
  assert.match(script, /curl -fL --retry 3/);
  assert.match(script, /wget -q -O/);
  assert.match(script, /sudo apt install -y/);
  assert.match(script, /sudo apt-get install -y/);
  assert.match(script, /sha256sum -c -/);
  assert.ok(script.includes(plan.asset.sha256));
  assert.ok(script.includes(plan.candidates[0]));
  assert.ok(script.includes(plan.candidates[1]));
  assert.ok(!script.includes("curl | sh"));
});

test("Debian script omits the fallback when only one source exists", () => {
  const script = debianInstallScript({ ...plan, candidates: [plan.candidates[0]] });
  assert.ok(!script.includes("|| download"));
});

test("Arch script uses the real AUR package and helpers", () => {
  const script = archInstallScript();
  assert.match(script, /yay -S --needed your-szu-life-bin/);
  assert.match(script, /paru -S --needed your-szu-life-bin/);
  assert.match(script, /aur\.archlinux\.org\/your-szu-life-bin/);
  assert.match(script, /makepkg -si/);
  assert.ok(!script.includes("your-szu-life-bin-bin"));
});

test("installCommandFor returns user-action commands for both families", () => {
  const debian = installCommandFor(plan, {
    family: "debian",
    prettyName: "Ubuntu 26.04",
  });
  assert.equal(debian.action, "run-install-command");
  assert.equal(debian.packageManager, "apt");
  assert.equal(debian.needsUserAction, true);
  assert.match(debian.script, /sudo apt install/);

  const arch = installCommandFor(plan, {
    family: "arch",
    prettyName: "Arch Linux",
  });
  assert.equal(arch.action, "run-install-command");
  assert.equal(arch.packageManager, "aur");
  assert.equal(arch.package, "your-szu-life-bin");
  assert.match(arch.script, /your-szu-life-bin/);
});
