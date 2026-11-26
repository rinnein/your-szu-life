import test from "node:test";
import assert from "node:assert/strict";

import {
  buildDownloadCandidates,
  compareVersions,
  extractVersionFromAsset,
  normalizeReleases,
  parseGithubReleaseHtml,
  selectRelease,
} from "../plugins/your-szu-life/bin/lib/releases.mjs";

const asset = (name, url, sha) => ({
  name,
  browser_download_url: url,
  hash_algo: sha ? "sha256" : undefined,
  hash_value: sha,
  size: 123,
});

const releases = [
  {
    tag_name: "v0.18.0",
    draft: false,
    prerelease: false,
    assets: [
      asset(
        "your-szu-life_windows_x64_0.18.0-release.msi",
        "https://cnb.cool/repo/msi",
        "1".repeat(64),
      ),
      asset(
        "your-szu-life_linux_x64_0.18.0-release.deb",
        "https://cnb.cool/repo/deb",
        "2".repeat(64),
      ),
      asset(
        "your-szu-life_linux_x64_0.18.0-release.AppImage",
        "https://cnb.cool/repo/appimage",
        "3".repeat(64),
      ),
    ],
  },
  {
    tag_name: "v0.17.0",
    draft: false,
    prerelease: false,
    assets: [
      asset(
        "your-szu-life_linux_x64_0.17.0-release.AppImage",
        "https://cnb.cool/repo/old-appimage",
        "4".repeat(64),
      ),
    ],
  },
  {
    tag_name: "canary",
    draft: false,
    prerelease: true,
    assets: [
      asset(
        "your-szu-life_linux_x64_0.19.0-release.AppImage",
        "https://cnb.cool/repo/canary-appimage",
        "5".repeat(64),
      ),
    ],
  },
  {
    tag_name: "announcement",
    assets: [],
  },
];

test("normalizeReleases accepts arrays and wrappers", () => {
  const array = normalizeReleases(releases);
  assert.equal(array.length, 3);
  assert.equal(array[0].assets[0].sha256, "1".repeat(64));
  const wrapped = normalizeReleases({ releases });
  assert.equal(wrapped.length, 3);
  assert.equal(wrapped[0].tag, "v0.18.0");
});

test("selectRelease prefers the newest stable asset for each format", () => {
  const normalized = normalizeReleases(releases);
  const linuxAuto = selectRelease(normalized, {
    platform: "linux",
    arch: "x64",
    channel: "stable",
    format: "auto",
  });
  assert.equal(linuxAuto.version, "0.18.0");
  assert.equal(linuxAuto.format, "appimage");

  const debian = selectRelease(normalized, {
    platform: "linux",
    arch: "x64",
    channel: "stable",
    format: "deb",
  });
  assert.equal(debian.format, "deb");
  assert.equal(debian.version, "0.18.0");

  const windows = selectRelease(normalized, {
    platform: "windows",
    arch: "x64",
    channel: "stable",
    format: "msi",
  });
  assert.equal(windows.format, "msi");
  assert.equal(windows.version, "0.18.0");
});

test("selectRelease falls back to canary and marks it", () => {
  const stableWithoutLinux = normalizeReleases([
    {
      tag_name: "v0.18.0",
      prerelease: false,
      assets: [asset("your-szu-life_windows_x64_0.18.0-release.msi", "u")],
    },
    releases[2],
  ]);
  const selected = selectRelease(stableWithoutLinux, {
    platform: "linux",
    arch: "x64",
    channel: "stable",
    format: "appimage",
  });
  assert.equal(selected.channel, "canary");
  assert.equal(selected.fallback, true);
  assert.equal(selected.version, "0.19.0");
});

test("canary channel prefers canary", () => {
  const normalized = normalizeReleases(releases);
  const selected = selectRelease(normalized, {
    platform: "linux",
    arch: "x64",
    channel: "canary",
    format: "appimage",
  });
  assert.equal(selected.channel, "canary");
  assert.equal(selected.version, "0.19.0");
});

test("asset version extraction and comparison", () => {
  assert.equal(
    extractVersionFromAsset("your-szu-life_linux_x64_0.18.1-release.AppImage"),
    "0.18.1",
  );
  assert.equal(extractVersionFromAsset("garbage"), null);
  assert.equal(compareVersions("0.19.0", "0.18.9"), 1);
  assert.equal(compareVersions("0.18.0", "0.18.0"), 0);
  assert.equal(compareVersions("0.17.9", "0.18.0"), -1);
});

test("download candidates put CNB first and deduplicate", () => {
  const selected = {
    release: { tag: "v0.18.0" },
    asset: {
      name: "your-szu-life_linux_x64_0.18.0-release.AppImage",
      url: "https://cnb.cool/repo/appimage",
    },
  };
  const candidates = buildDownloadCandidates(selected);
  assert.ok(candidates[0].includes("cnb.cool"));
  assert.ok(candidates.some((url) => url.startsWith("https://gh-proxy.com/")));
  assert.ok(candidates.some((url) => url.startsWith("https://ghfast.top/")));
  assert.equal(new Set(candidates).size, candidates.length);
  assert.ok(
    candidates.some((url) =>
      url.startsWith("https://github.com/rinnein/your-szu-life/releases/download/"),
    ),
  );
});

test("GitHub HTML fallback parses release assets", () => {
  const html = `
    <a href="/rinnein/your-szu-life/releases/tag/v0.18.0">v0.18.0</a>
    <a href="/rinnein/your-szu-life/releases/download/v0.18.0/your-szu-life_linux_x64_0.18.0-release.AppImage">a</a>
    <a href="/rinnein/your-szu-life/releases/download/v0.18.0/your-szu-life_windows_x64_0.18.0-release.msi">b</a>
  `;
  const parsed = parseGithubReleaseHtml(html);
  assert.equal(parsed.length, 1);
  assert.equal(parsed[0].tag, "v0.18.0");
  assert.equal(parsed[0].assets.length, 2);
});
