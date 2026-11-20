import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, LoadingIndicator, TextField } from "material/react";
import { AdaptiveTabs, Banner, Chip, bindField } from "../../ui";
import {
  taPhase,
  taPhaseName,
  scoredStatusName,
  type ManagedContest,
  type ManagedContests,
  type Site,
} from "../types";
import { taContestsKey } from "./ta-cache";
import { useTaList } from "./useTaList";

const phases = [
  { value: "all", label: "全部" },
  { value: "0", label: "未开始" },
  { value: "1", label: "进行中" },
  { value: "2", label: "已结束" },
];

export function TaWorkbench({
  site,
  onOpenContest,
  registerRefresh,
}: {
  site: Site;
  onOpenContest: (contest: ManagedContest) => void;
  registerRefresh?: (refresh: () => void) => void;
}) {
  const cacheKey = taContestsKey(site.id, site.username, site.sessionGeneration);
  const { state, refresh } = useTaList<ManagedContests>({
    cacheKey,
    load: () => invoke<ManagedContests>("get_oj_managed_contests", { siteId: site.id }),
  });
  const [query, setQuery] = useState("");
  const [phase, setPhase] = useState("all");
  useEffect(() => {
    registerRefresh?.(() => void refresh());
  }, [refresh, registerRefresh]);
  const data =
    state.status === "ready"
      ? state.data.data
      : state.status === "loading"
        ? state.previous?.data
        : state.status === "error"
          ? state.previous?.data
          : undefined;
  const updatedAt =
    state.status === "ready"
      ? state.data.updatedAt
      : state.status === "loading"
        ? state.previous?.updatedAt
        : state.status === "error"
          ? state.previous?.updatedAt
          : undefined;
  const rows = useMemo(() => {
    if (!data) return [];
    const text = query.trim().toLowerCase();
    return data.rows.filter((contest) => {
      if (text && !`${contest.title} ${contest.groupName}`.toLowerCase().includes(text)) {
        return false;
      }
      return phase === "all" || taPhase(contest, data.now) === Number(phase);
    });
  }, [data, phase, query]);

  return (
    <section className="oj-ta-workbench">
      <div className="oj-actions oj-ta-toolbar">
        <TextField
          label="查找测验"
          value={query}
          {...bindField(setQuery)}
          placeholder="测验名称或班级"
        />
        <AdaptiveTabs
          items={phases}
          value={phase}
          onChange={setPhase}
          variant="segmented"
          size="sm"
          label="筛选阶段"
          ariaLabel="筛选测验阶段"
        />
        <Button
          variant="tonal"
          disabled={state.status === "loading"}
          onClick={() => void refresh()}
        >
          刷新
        </Button>
      </div>
      {state.status === "error" && !data && (
        <Banner status="warning">
          <Banner.Title>助教测验读取失败</Banner.Title>
          <Banner.Description>{state.message}</Banner.Description>
        </Banner>
      )}
      {(state.status === "loading" || state.status === "idle") && !data && (
        <LoadingIndicator size={24} aria-label="正在读取助教测验" />
      )}
      {updatedAt && (
        <p className="oj-muted">
          列表更新于 {new Date(updatedAt).toLocaleString()}
          {state.status === "loading" ? " · 正在刷新" : ""}
        </p>
      )}
      {data && rows.length === 0 && <p className="oj-muted">没有匹配的可管理测验。</p>}
      <div className="oj-site-grid">
        {rows.map((contest) => {
          const stage = taPhase(contest, data!.now);
          return (
            <Card
              key={contest.id}
              variant="outlined"
              headline={contest.title}
              subhead={`${contest.groupName || `班级 ${contest.groupId}`} · ${
                contest.isExam ? "考试" : "测验"
              }`}
            >
              <div className="oj-ta-contest-body">
                <div className="oj-actions">
                  <Chip color={stage === 1 ? "accent" : "default"}>{taPhaseName[stage]}</Chip>
                  {contest.scoredStatus && (
                    <Chip
                      color={
                        contest.scoredStatus === "done"
                          ? "success"
                          : contest.scoredStatus === "running"
                            ? "warning"
                            : "default"
                      }
                    >
                      {scoredStatusName[contest.scoredStatus] ?? contest.scoredStatus}
                    </Chip>
                  )}
                  {contest.totalScore != null && (
                    <span className="oj-muted">总分 {contest.totalScore}</span>
                  )}
                </div>
                <p className="oj-muted">
                  {new Date(contest.begin).toLocaleString()} 至{" "}
                  {new Date(contest.end).toLocaleString()}
                </p>
              </div>
              <div slot="actions">
                <Button onClick={() => onOpenContest(contest)}>进入批阅</Button>
              </div>
            </Card>
          );
        })}
      </div>
    </section>
  );
}
