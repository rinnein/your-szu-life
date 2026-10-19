import type { CommandItem, CommandUsage } from "./types";

const HISTORY_KEY = "command_palette_history";

let lastTimestamp = 0;
function uniqueNow(): number {
  const now = Date.now();
  lastTimestamp = Math.max(now, lastTimestamp + 1);
  return lastTimestamp;
}

export function getCommandHistory(): Record<string, CommandUsage> {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (raw) {
      return JSON.parse(raw);
    }
  } catch {
    // ignore localStorage errors in restricted environments
  }
  return {};
}

export function recordCommandUsage(commandId: string): void {
  try {
    const history = getCommandHistory();
    const existing = history[commandId] ?? { id: commandId, useCount: 0, lastUsedAt: 0 };
    history[commandId] = {
      id: commandId,
      useCount: existing.useCount + 1,
      lastUsedAt: uniqueNow(),
    };
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history));
  } catch {
    // ignore
  }
}

export function getRecentCommands(commands: CommandItem[], limit = 4): CommandItem[] {
  const history = getCommandHistory();
  const commandMap = new Map(commands.map((c) => [c.id, c]));

  return Object.values(history)
    .filter((h) => h.lastUsedAt > 0 && commandMap.has(h.id))
    .sort((a, b) => b.lastUsedAt - a.lastUsedAt)
    .slice(0, limit)
    .map((h) => commandMap.get(h.id)!);
}

export function getFrequentCommands(commands: CommandItem[], limit = 4): CommandItem[] {
  const history = getCommandHistory();
  const commandMap = new Map(commands.map((c) => [c.id, c]));

  return Object.values(history)
    .filter((h) => h.useCount > 0 && commandMap.has(h.id))
    .sort((a, b) => b.useCount - a.useCount)
    .slice(0, limit)
    .map((h) => commandMap.get(h.id)!);
}

export function getRecommendedCommands(commands: CommandItem[], limit = 6): CommandItem[] {
  const history = getCommandHistory();
  const now = Date.now();
  const dayMs = 24 * 60 * 60 * 1000;

  // Calculate score for each command based on frequency and recency
  const scored = commands.map((cmd) => {
    const usage = history[cmd.id];
    if (!usage) {
      return { cmd, score: 0 };
    }
    const daysAgo = Math.max(0, (now - usage.lastUsedAt) / dayMs);
    const recencyWeight = Math.max(0, 50 - daysAgo * 5); // decay over 10 days
    const frequencyWeight = usage.useCount * 15;
    return { cmd, score: recencyWeight + frequencyWeight };
  });

  scored.sort((a, b) => b.score - a.score);

  // If user has history, return top scored items
  const withUsage = scored.filter((s) => s.score > 0);
  if (withUsage.length > 0) {
    return withUsage.slice(0, limit).map((s) => s.cmd);
  }

  // Default recommendations if no history yet
  const defaultIds = ["time", "oj", "uooc", "cdl", "cdf", "net"];
  const defaults = defaultIds
    .map((id) => commands.find((c) => c.id === id))
    .filter((c): c is CommandItem => Boolean(c));

  return defaults.slice(0, limit);
}
