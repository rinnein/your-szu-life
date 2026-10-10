import { useEffect, useMemo, useState } from "react";
import { useSelector } from "@tanstack/react-store";
import {
  EMPTY_PROGRESS,
  ojProgressCache,
  progressScopeKey,
  type ProgressScope,
} from "./progress-store";
import { Button, Chip, Table } from "@heroui/react";
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
    <span className="inline-flex flex-wrap gap-2">
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
  return (
    <Table>
      <Table.ScrollContainer>
        <Table.Content aria-label="测验列表" className="min-w-[720px]">
          <Table.Header>
            <Table.Column isRowHeader>名称</Table.Column>
            <Table.Column>状态</Table.Column>
            <Table.Column>开始时间</Table.Column>
            <Table.Column>截止时间</Table.Column>
            <Table.Column>操作</Table.Column>
          </Table.Header>
          <Table.Body renderEmptyState={() => "暂无匹配的测验"}>
            {contests
              .filter((c) => c.title.includes(query))
              .sort((a, b) => b.begin - a.begin)
              .map((contest) => {
                // Anchor each response's server clock instead of trusting the device timezone/clock.
                const now = contest.now + Math.max(0, clock - sampledAt);
                const state = phase(contest, now);
                const active = state === "进行中";
                return (
                  <Table.Row
                    key={contest.id}
                    id={contest.id}
                    className={active ? "oj-contest-active" : undefined}
                  >
                    <Table.Cell>{contest.title}</Table.Cell>
                    <Table.Cell>
                      <ContestStatus
                        contest={contest}
                        now={now}
                        complete={
                          (progress[contest.id]?.value?.total ?? 0) > 0 &&
                          progress[contest.id].value?.accepted === progress[contest.id].value?.total
                        }
                      />
                    </Table.Cell>
                    <Table.Cell>{new Date(contest.begin).toLocaleString()}</Table.Cell>
                    <Table.Cell>
                      {new Date(contest.end).toLocaleString()}
                      {active && (
                        <span className="oj-contest-countdown">
                          （剩余 {formatRemaining(contest.end - now)}）
                        </span>
                      )}
                    </Table.Cell>
                    <Table.Cell>
                      <Button
                        variant={state === "已结束" ? "secondary" : "primary"}
                        isDisabled={state === "未开始"}
                        onPress={() => onOpen(contest)}
                      >
                        {state === "已结束" ? "查看测验" : "进入测验"}
                      </Button>
                    </Table.Cell>
                  </Table.Row>
                );
              })}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}
