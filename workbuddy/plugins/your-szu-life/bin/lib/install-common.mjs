// Shared download + verification helpers for the Linux and Windows installers.

import { rm } from "node:fs/promises";

import { downloadFile, fileSize, hasMagic, log, sha256File } from "./util.mjs";

export async function isElf(target) {
  return hasMagic(target, [0x7f, 0x45, 0x4c, 0x46]);
}

export async function isDeb(target) {
  return hasMagic(target, [0x21, 0x3c, 0x61, 0x72, 0x63, 0x68, 0x3e]);
}

export async function isOle(target) {
  return hasMagic(target, [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
}

export function verifierFor(kind) {
  if (kind === "elf") return isElf;
  if (kind === "deb") return isDeb;
  if (kind === "ole") return isOle;
  return async () => true;
}

/**
 * Try each candidate URL until one downloads and verifies. Returns the URL and
 * downloader used. The destination is left in place for the caller to install.
 */
export async function downloadVerified(candidates, destination, options = {}) {
  const { kind = "elf", sha256, minSize = 5 * 1024 * 1024, progress = true } = options;
  const verify = verifierFor(kind);
  const errors = [];

  for (const [index, url] of candidates.entries()) {
    try {
      if (progress) {
        log(`下载 (${index + 1}/${candidates.length}): ${url}`);
      }
      const tool = await downloadFile(url, destination);
      const size = await fileSize(destination);
      if (size < minSize) {
        throw new Error(`文件过小 (${size} bytes)`);
      }
      if (!(await verify(destination))) {
        throw new Error("文件类型校验失败");
      }
      if (sha256) {
        const actual = await sha256File(destination);
        if (actual.toLowerCase() !== sha256.toLowerCase()) {
          throw new Error(`SHA-256 不匹配（期望 ${sha256}，实际 ${actual}）`);
        }
      }
      return { url, tool, size };
    } catch (error) {
      errors.push(`${url}: ${String(error)}`);
      await rm(destination, { force: true }).catch(() => {});
    }
  }

  throw new Error(`所有下载源均失败：\n${errors.join("\n")}`);
}
