import type { Gear } from "./model";
export type Preferences = { gear: Gear; startAt: string };
const key = "szu.enrollment.preferences.v1";
export function loadPreferences(): Preferences {
  try {
    const p = JSON.parse(localStorage.getItem(key) ?? "{}");
    return {
      gear: ["low", "medium", "high", "ultra"].includes(p.gear) ? p.gear : "medium",
      startAt: typeof p.startAt === "string" ? p.startAt : "",
    };
  } catch {
    return { gear: "medium", startAt: "" };
  }
}
export function savePreferences(p: Preferences) {
  localStorage.setItem(key, JSON.stringify(p));
}
