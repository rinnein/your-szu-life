import { createMemoryHistory } from "@tanstack/react-router";
import { initialLogicalEntries, isChildDocument } from "./logical-navigation";

export const LOGICAL_HISTORY_KEY = "szu.logical-history.v1";
type Snapshot = { version: 1; child: boolean; entries: string[]; index: number };
type Storage = Pick<globalThis.Storage, "getItem" | "setItem">;

export function consumeAuthReturn(
  storage: Pick<globalThis.Storage, "getItem" | "removeItem"> | undefined,
  href: string,
) {
  try {
    const saved = storage?.getItem("szu.auth-return.v1");
    storage?.removeItem("szu.auth-return.v1");
    return saved === href;
  } catch {
    return false;
  }
}

function restore(raw: string | null, href: string): Snapshot | undefined {
  if (!raw || raw.length > 128_000) return;
  const value = JSON.parse(raw) as Snapshot;
  const current = new URL(href);
  current.searchParams.delete("__szu_window");
  if (
    value?.version !== 1 ||
    value.child !== isChildDocument(href) ||
    !Array.isArray(value.entries) ||
    !value.entries.length ||
    !Number.isInteger(value.index) ||
    value.index < 0 ||
    value.index >= value.entries.length ||
    value.entries.some((entry) => {
      if (typeof entry !== "string" || !entry.startsWith("/") || entry.startsWith("//"))
        return true;
      const url = new URL(entry, current.origin);
      return url.origin !== current.origin || url.searchParams.has("__szu_window");
    }) ||
    value.entries[value.index] !== current.pathname + current.search + current.hash ||
    (!value.child && value.entries[0] !== "/")
  )
    return;
  return value;
}

/** Store route paths only, in this WebView's session; never persist form/auth data. */
export function createReloadableLogicalHistory(options: {
  href: string;
  reload: boolean;
  storage?: Storage;
}) {
  let snapshot: Snapshot | undefined;
  try {
    if (options.reload)
      snapshot = restore(options.storage?.getItem(LOGICAL_HISTORY_KEY) ?? null, options.href);
  } catch {
    // Unavailable/corrupt storage uses the normal deep-link parent context.
  }
  let entries = snapshot?.entries ?? initialLogicalEntries(options.href);
  let index = snapshot?.index ?? entries.length - 1;
  // Keep our own copy: TanStack's memory history mutates its input entries.
  const history = createMemoryHistory({ initialEntries: [...entries], initialIndex: index });
  // The installed history treats initialIndex=0 as absent. Explicitly restore
  // the root cursor when a reload follows back navigation with forward entries.
  if (history.location.state.__TSR_index !== index)
    history.go(index - history.location.state.__TSR_index);
  const save = () => {
    try {
      options.storage?.setItem(
        LOGICAL_HISTORY_KEY,
        JSON.stringify({
          version: 1,
          child: isChildDocument(options.href),
          entries,
          index,
        } satisfies Snapshot),
      );
    } catch {
      // Storage failure must not block normal navigation or a native reload.
    }
  };
  history.subscribe(({ location, action }) => {
    index = location.state.__TSR_index;
    if (action.type === "PUSH") entries = [...entries.slice(0, index), location.href];
    else if (action.type === "REPLACE") entries[index] = location.href;
    save();
  });
  save();
  return history;
}
