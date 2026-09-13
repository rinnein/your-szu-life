/** Copy locally, falling back to selection for older / restricted WebViews. */
export async function copyText(value: string): Promise<boolean> {
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
