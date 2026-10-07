import { useLocation } from "@tanstack/react-router";
import type { Snapshot } from "./model";
export function useCacheMode() {
  return useLocation({ select: (l) => new URLSearchParams(l.searchStr).get("view") === "cache" });
}
export function cachePath(path: string) {
  const url = new URL(path, "https://local.invalid");
  url.searchParams.set("view", "cache");
  return url.pathname + url.search + url.hash;
}
export function livePath(path: string) {
  const url = new URL(path, "https://local.invalid");
  url.searchParams.delete("view");
  return url.pathname + url.search + url.hash;
}
export function canSelect(snapshot: Snapshot | undefined) {
  return (
    !!snapshot?.loggedIn &&
    (snapshot.authentication ?? "authenticated") === "authenticated" &&
    (snapshot.admission ?? "ready") === "ready"
  );
}
