import { useEffect, useMemo, useState } from "react";
import { useSelector } from "@tanstack/react-store";
import {
  EMPTY_PROGRESS,
  ojProgressCache,
  progressScopeKey,
  type ProgressScope,
} from "./progress-store";
import { List, ListItem } from "material/react";
import { Chip } from "../ui";
import { type Contest, phase } from "./types";

export function ContestStatus({
  contest,
  now,
  complete = false,
}: {
  contest: Contest;
  now: number;
  complete?: boolean;
}) {
  const state = phase(contest, now);
  return (
    <span className="inline-flex items-center flex-nowrap gap-2 whitespace-nowrap">
      <Chip
        variant="primary"
        color={state === "进行中" ? "success" : state === "已结束" ? "danger" : "default"}
      >
        {state}
      </Chip>
      {complete && (
        <Chip variant="primary" color="success">
          已完成
        </Chip>
      )}
    </span>
  );
}

export function formatRemaining(milliseconds: number): string {
  const total = Math.max(0, Math.ceil(milliseconds / 1000));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  const clock = [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":");
  return days ? `${days}天 ${clock}` : clock;
}

/**
 * 测验列表。
 *
 * 每场测验的字段数（名称、状态、开始/截止、操作）对表格来说偏少，而窄屏下
 * 表格必然横向滚动；改用 MD3 的 List：名称是行标题，时间与状态放在支持文本里，
 * 「进入/查看」作为尾部按钮。整行可点，与浏览器的列表习惯一致。
 */
export function ContestTable({
  contests,
  query,
  onOpen,
  siteId,
  account,
  siteUrl = "",
  sessionGeneration = null,
}: {
  contests: Contest[];
  query: string;
  onOpen: (contest: Contest) => void;
  siteId?: string;
  account?: string;
  siteUrl?: string;
  sessionGeneration?: number | null;
}) {
  const scope: ProgressScope = {
    siteId: siteId ?? "",
    account: account ?? "",
    url: siteUrl,
    generation: sessionGeneration,
  };
  const key = progressScopeKey(scope);
  const progress = useSelector(
    ojProgressCache.store,
    (state) => state[key]?.entries ?? EMPTY_PROGRESS,
  );
  const revision = useSelector(ojProgressCache.store, (state) => state[key]?.revision ?? 0);
  useEffect(() => {
    let active = true;
    if (siteId && account)
      void (async () => {
        for (const contest of contests) {
          if (!active) return;
          // Only running contests can still change; completed status for
          // started/ended ones comes from the cache (1h TTL) or manual refresh.
          if (phase(contest, contest.now) !== "进行中") continue;
          await ojProgressCache.ensure(scope, contest.id);
        }
      })();
    // In-flight requests belong to the shared cache, not this page's lifetime.
    return () => {
      active = false;
    };
  }, [key, contests, revision]);
  const sampledAt = useMemo(() => Date.now(), [contests]);
  const [clock, setClock] = useState(Date.now);
  useEffect(() => {
    const tick = () => setClock(Date.now());
    const timer = setInterval(tick, 1000);
    document.addEventListener("visibilitychange", tick);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", tick);
    };
  }, []);

  const visible = contests.filter((c) => c.title.includes(query)).sort((a, b) => b.begin - a.begin);
  const byId = new Map(visible.map((contest) => [contest.id, contest]));
  // 进行中的测验用列表的选中态表达：M3 的 List 只提供这一种行级强调，
  // 而它正好对应「当前可进入」的语义。
  const running = visible.find(
    (contest) => phase(contest, contest.now + Math.max(0, clock - sampledAt)) === "进行中",
  );

  return (
    <List
      aria-label="测验列表"
      selection="single"
      className="oj-contest-list"
      /* 传空串而不是 undefined：后者会被 React 当作移除属性，元素收不到更新。 */
      value={running?.id ?? ""}
      onActivate={(event) => {
        const contest = byId.get(event.detail.value);
        if (contest) onOpen(contest);
      }}
    >
      {visible.map((contest) => {
        // Anchor each response's server clock instead of trusting the device timezone/clock.
        const now = contest.now + Math.max(0, clock - sampledAt);
        const state = phase(contest, now);
        const active = state === "进行中";
        const complete =
          (progress[contest.id]?.value?.total ?? 0) > 0 &&
          progress[contest.id].value?.accepted === progress[contest.id].value?.total;
        return (
          <ListItem
            key={contest.id}
            value={contest.id}
            overline={complete ? "已完成" : undefined}
            headline={contest.title}
            supportingText={
              active
                ? `${state} · 截止 ${new Date(contest.end).toLocaleString()}（剩余 ${formatRemaining(
                    contest.end - now,
                  )}）`
                : `${state} · ${new Date(contest.begin).toLocaleString()} — ${new Date(
                    contest.end,
                  ).toLocaleString()}`
            }
            trailingText={
              state === "已结束" ? "查看测验" : state === "未开始" ? "未开始" : "进入测验"
            }
          />
        );
      })}
      {visible.length === 0 && <ListItem value="__empty" headline="暂无匹配的测验" disabled />}
    </List>
  );
}
