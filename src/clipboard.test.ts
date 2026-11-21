/** @vitest-environment jsdom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { copyRichText, copyText } from "./clipboard";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
  writeHtml: vi.fn(),
}));

const originalClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
const originalExecCommand = Object.getOwnPropertyDescriptor(document, "execCommand");

function setClipboard(value: unknown) {
  Object.defineProperty(navigator, "clipboard", { configurable: true, value });
}

function setFallback(value: unknown) {
  Object.defineProperty(document, "execCommand", { configurable: true, value });
}

describe("shared WebView clipboard", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    document.getSelection()?.removeAllRanges();
    setClipboard(undefined);
    setFallback(undefined);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    document.body.replaceChildren();
    if (originalClipboard) Object.defineProperty(navigator, "clipboard", originalClipboard);
    else Reflect.deleteProperty(navigator, "clipboard");
    if (originalExecCommand) Object.defineProperty(document, "execCommand", originalExecCommand);
    else Reflect.deleteProperty(document, "execCommand");
  });

  it("prefers Clipboard API without creating a fallback selection", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    const fallback = vi.fn();
    setClipboard({ writeText });
    setFallback(fallback);
    await expect(copyText("https://example.test/?first=1&second=two")).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith("https://example.test/?first=1&second=two");
    expect(fallback).not.toHaveBeenCalled();
    expect(document.querySelector("textarea")).toBeNull();
  });

  it.each(["missing", "denied"])(
    "falls back when Clipboard API is %s and restores focus / selection",
    async (mode) => {
      setClipboard(
        mode === "missing"
          ? undefined
          : { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
      );
      const button = document.createElement("button");
      const paragraph = document.createElement("p");
      paragraph.textContent = "keep this selection";
      document.body.append(button, paragraph);
      button.focus();
      const range = document.createRange();
      range.selectNodeContents(paragraph);
      document.getSelection()?.removeAllRanges();
      document.getSelection()?.addRange(range);
      const fallback = vi.fn(() => {
        const textarea = document.querySelector("textarea");
        expect(textarea?.value).toBe("complete text\nsecond line");
        expect(textarea?.selectionStart).toBe(0);
        expect(textarea?.selectionEnd).toBe(textarea?.value.length);
        return true;
      });
      setFallback(fallback);
      await expect(copyText("complete text\nsecond line")).resolves.toBe(true);
      expect(fallback).toHaveBeenCalledWith("copy");
      expect(document.querySelector("textarea")).toBeNull();
      expect(document.activeElement).toBe(button);
      expect(document.getSelection()?.toString()).toBe("keep this selection");
    },
  );

  it.each(["false", "throw", "unavailable"])(
    "does not report success when fallback is %s",
    async (mode) => {
      setClipboard({ writeText: vi.fn().mockRejectedValue(new Error("denied")) });
      const fallback =
        mode === "unavailable"
          ? undefined
          : vi.fn(() => {
              if (mode === "throw") throw new Error("unsupported");
              return false;
            });
      setFallback(fallback);
      await expect(copyText("manual copy remains available")).resolves.toBe(false);
      expect(document.querySelector("textarea")).toBeNull();
    },
  );
  it("reports failure instead of rejecting if selection APIs are unavailable", async () => {
    vi.spyOn(document, "getSelection").mockImplementation(() => {
      throw new Error("unsupported selection");
    });
    await expect(copyText("manual copy remains available")).resolves.toBe(false);
    expect(document.querySelector("textarea")).toBeNull();
  });

  it("keeps a successful native copy result even if restoring focus fails", async () => {
    const button = document.createElement("button");
    document.body.appendChild(button);
    button.focus();
    vi.spyOn(button, "focus").mockImplementation(() => {
      throw new Error("detached focus");
    });
    setFallback(vi.fn(() => true));
    await expect(copyText("complete text")).resolves.toBe(true);
    expect(document.querySelector("textarea")).toBeNull();
  });
});

describe("rich text copy", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    document.getSelection()?.removeAllRanges();
    setClipboard(undefined);
    setFallback(undefined);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    document.body.replaceChildren();
    if (originalClipboard) Object.defineProperty(navigator, "clipboard", originalClipboard);
    else Reflect.deleteProperty(navigator, "clipboard");
    if (originalExecCommand) Object.defineProperty(document, "execCommand", originalExecCommand);
    else Reflect.deleteProperty(document, "execCommand");
  });

  it("writes both flavors through the Tauri plugin on desktop", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    const writeHtml = vi.mocked((await import("@tauri-apps/plugin-clipboard-manager")).writeHtml);
    writeHtml.mockResolvedValue(undefined);
    await expect(copyRichText("plain", "<b>rich</b>")).resolves.toEqual({ ok: true, rich: true });
    expect(writeHtml).toHaveBeenCalledWith("<b>rich</b>", "plain");
  });

  it("falls back to the Clipboard API when the plugin rejects", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    vi.mocked((await import("@tauri-apps/plugin-clipboard-manager")).writeHtml).mockRejectedValue(
      new Error("not supported"),
    );
    const write = vi.fn().mockResolvedValue(undefined);
    setClipboard({ write });
    vi.stubGlobal(
      "ClipboardItem",
      class {
        constructor(public formats: Record<string, Blob>) {}
      },
    );
    await expect(copyRichText("plain", "<b>rich</b>")).resolves.toEqual({ ok: true, rich: true });
    expect(write).toHaveBeenCalledTimes(1);
    const [items] = write.mock.calls[0] as [Array<{ formats: Record<string, Blob> }>];
    expect(items).toHaveLength(1);
    expect(await items[0].formats["text/plain"].text()).toBe("plain");
    expect(await items[0].formats["text/html"].text()).toBe("<b>rich</b>");
  });

  it("degrades to plain text when no HTML writer is available", async () => {
    setFallback(vi.fn(() => true));
    await expect(copyRichText("plain", "<b>rich</b>")).resolves.toEqual({ ok: true, rich: false });
  });

  it("reports failure when every writer fails", async () => {
    setFallback(vi.fn(() => false));
    await expect(copyRichText("plain", "<b>rich</b>")).resolves.toEqual({ ok: false, rich: false });
  });
});
