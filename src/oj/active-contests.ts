import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ojProgressCache, progressScope } from "./progress-store";
import type { Contest, Group, Snapshot } from "./types";

export interface ActiveOjContest {
  siteId: string;
  siteName: string;
  contestId: string;
  groupId?: string;
  title: string;
  begin: number;
  end: number;
  now?: number;
  url: string;
}

export function useActiveOjContests(): ActiveOjContest[] {
  const [activeContests, setActiveContests] = useState<ActiveOjContest[]>([]);

  useEffect(() => {
    let active = true;

    const check = async () => {
      try {
        const snapshot = await invoke<Snapshot>("get_oj_snapshot");
        if (!active || !snapshot?.sites) return;

        const results: ActiveOjContest[] = [];
        const seen = new Set<string>();
        const now = Date.now();

        for (const site of snapshot.sites) {
          if (!active) return;
          if (!site.enabled || site.session !== "ready") continue;

          let groups: Group[] = [];
          try {
            groups = await invoke<Group[]>("get_oj_groups", { siteId: site.id });
          } catch {
            continue;
          }

          for (const group of groups) {
            if (!active) return;
            let contests: Contest[] = [];
            try {
              contests = await invoke<Contest[]>("get_oj_contests", {
                siteId: site.id,
                groupId: group.id,
              });
            } catch {
              continue;
            }

            for (const contest of contests) {
              if (!active) return;
              const contestKey = `${site.id}:${contest.id}`;
              if (seen.has(contestKey)) continue;

              const contestNow = contest.now || now;
              const isRunning = contestNow >= contest.begin && contestNow < contest.end;
              if (!isRunning) continue;

              let isCompleted = false;
              try {
                const scope = progressScope(site);
                const progress = await ojProgressCache.ensure(scope, contest.id);
                if (progress && progress.total > 0 && progress.accepted === progress.total) {
                  isCompleted = true;
                }
              } catch {
                // If progress fails to fetch, keep as incomplete
              }

              if (!isCompleted) {
                seen.add(contestKey);
                results.push({
                  siteId: site.id,
                  siteName: site.name,
                  contestId: contest.id,
                  groupId: group.id,
                  title: contest.title,
                  begin: contest.begin,
                  end: contest.end,
                  now: contest.now,
                  url: `/oj?site=${encodeURIComponent(site.id)}&contest=${encodeURIComponent(contest.id)}`,
                });
              }
            }
          }
        }

        if (active) {
          setActiveContests(results);
        }
      } catch {
        // ignore errors
      }
    };

    void check();

    const unlistenState = listen("oj-state-changed", () => void check());
    const unlistenProgress = listen("oj-progress-invalidated", () => void check());

    const interval = setInterval(() => void check(), 60_000);

    const onVisibility = () => {
      if (document.visibilityState === "visible") void check();
    };
    document.addEventListener("visibilitychange", onVisibility);

    return () => {
      active = false;
      clearInterval(interval);
      document.removeEventListener("visibilitychange", onVisibility);
      void unlistenState.then((fn) => fn());
      void unlistenProgress.then((fn) => fn());
    };
  }, []);

  return activeContests;
}
