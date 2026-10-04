import { useEffect, useMemo, useState } from "react";
import { Button, Chip, Table } from "@heroui/react";
import { type Contest, phase } from "./types";

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
}: {
  contests: Contest[];
  query: string;
  onOpen: (contest: Contest) => void;
}) {
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
                      <Chip color={active ? "success" : state === "已结束" ? "danger" : "default"}>
                        {state}
                      </Chip>
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
                      <Button isDisabled={state === "未开始"} onPress={() => onOpen(contest)}>
                        进入测验
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
