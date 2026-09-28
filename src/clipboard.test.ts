/** @vitest-environment jsdom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { copyText } from "./clipboard";

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
