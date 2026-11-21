import {
  writeHtml as tauriWriteHtml,
  writeText as tauriWriteText,
} from "@tauri-apps/plugin-clipboard-manager";

/** Copy locally using Tauri clipboard-manager plugin, falling back to Web API / selection for older / restricted WebViews. */
export async function copyText(value: string): Promise<boolean> {
  try {
    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      await tauriWriteText(value);
      return true;
    }
  } catch {
    // Plugin call failed or unavailable; fall through to browser APIs
  }

  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    // Permission failures must still try the selection-based fallback.
  }

  if (typeof document === "undefined") return false;
  let textarea: HTMLTextAreaElement | undefined;
  let restoreSelection: (() => void) | undefined;
  try {
    const previousFocus = document.activeElement;
    const selection = document.getSelection();
    const ranges = selection
      ? Array.from({ length: selection.rangeCount }, (_, index) =>
          selection.getRangeAt(index).cloneRange(),
        )
      : [];
    restoreSelection = () => {
      if (previousFocus instanceof HTMLElement) previousFocus.focus({ preventScroll: true });
      if (selection) {
        selection.removeAllRanges();
        for (const range of ranges) selection.addRange(range);
      }
    };
    textarea = document.createElement("textarea");
    textarea.value = value;
    textarea.readOnly = true;
    textarea.tabIndex = -1;
    textarea.setAttribute("aria-hidden", "true");
    textarea.style.cssText =
      "position:fixed;top:0;left:0;opacity:0;pointer-events:none;font-size:16px";
    document.body.appendChild(textarea);
    textarea.focus({ preventScroll: true });
    textarea.select();
    textarea.setSelectionRange(0, value.length);
    return document.execCommand("copy") === true;
  } catch {
    return false;
  } finally {
    try {
      textarea?.remove();
    } catch {
      // Cleanup is best-effort; the native copy result remains authoritative.
    }
    try {
      restoreSelection?.();
    } catch {
      // A detached element / unsupported focus option cannot change copy's result.
    }
  }
}

/**
 * 同时写入纯文本与 text/html 两种剪贴板 flavor：富文本目标（文档、聊天框）
 * 直接渲染 HTML（图片为内联 data URL），纯文本目标拿到原文。
 *
 * 尝试链：Tauri 插件的 `writeHtml`（桌面 arboard 一次写入双 flavor；Android/iOS
 * 平台层不支持）→ Web ClipboardItem 双 flavor（Android WebView 路径）→ 只复制
 * 纯文本。`rich` 标记是否成功写入 HTML flavor。
 */
export async function copyRichText(
  plain: string,
  html: string,
): Promise<{ ok: boolean; rich: boolean }> {
  try {
    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      await tauriWriteHtml(html, plain);
      return { ok: true, rich: true };
    }
  } catch {
    // Android/iOS 不支持或插件被拒；落到 Web API。
  }

  try {
    if (navigator.clipboard?.write && typeof ClipboardItem !== "undefined") {
      await navigator.clipboard.write([
        new ClipboardItem({
          "text/plain": new Blob([plain], { type: "text/plain" }),
          "text/html": new Blob([html], { type: "text/html" }),
        }),
      ]);
      return { ok: true, rich: true };
    }
  } catch {
    // WebView 不支持 HTML flavor 时降级为纯文本。
  }

  return { ok: await copyText(plain), rich: false };
}
