import { openUrl } from "@tauri-apps/plugin-opener";

export function publicWebUrl(value: string): string {
  const url = new URL(value);
  const authority = value.split("://")[1]?.split(/[/?#]/, 1)[0];
  if (
    value.trim() !== value ||
    value.length > 4096 ||
    [...value].some((character) => {
      const code = character.charCodeAt(0);
      return code <= 0x1f || code === 0x7f;
    }) ||
    !["http:", "https:"].includes(url.protocol) ||
    !url.hostname ||
    url.username ||
    url.password ||
    authority?.includes("@")
  ) {
    throw new Error("仅支持不含用户名和密码的 HTTP / HTTPS 地址");
  }
  return url.href;
}

export async function openPublicUrl(value: string) {
  await openUrl(publicWebUrl(value));
}

export function contentTime(timestamp: number | null | undefined) {
  return timestamp == null
    ? "尚未检查"
    : new Date(timestamp).toLocaleString("zh-CN", { hour12: false });
}
