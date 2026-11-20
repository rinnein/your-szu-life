import { invoke } from "@tauri-apps/api/core";
import { Button, Card, LoadingIndicator } from "material/react";
import { AdaptiveTabs, Banner, Chip } from "../../ui";
import { useAsyncLoad } from "../../async-state";
import {
  scoredStatusName,
  taPhase,
  taPhaseName,
  type ManagedContestDetail,
  type Site,
} from "../types";
import { TaProblemView } from "./TaProblemView";
import { TaReviewDetail } from "./TaReviewDetail";
import { TaReviewList } from "./TaReviewList";
import type { TaTab, TaView } from "./ta-route";

export interface TaNavigate {
  taView?: TaView;
  taTab?: TaTab;
  problem?: string;
  review?: string;
  codeSubmission?: string;
}

export function TaContest({
  site,
  contestId,
  taView,
  taTab,
  problem,
  review,
  codeSubmission,
  desktop,
  onNavigate,
  onOpenStudent,
}: {
  site: Site;
  contestId: string;
  taView?: TaView;
  taTab?: TaTab;
  problem?: string;
  review?: string;
  codeSubmission?: string;
  desktop: boolean;
  onNavigate: (next: TaNavigate) => void;
  onOpenStudent: (problemKey: string) => void;
}) {
  const { state, reload } = useAsyncLoad(() =>
    invoke<ManagedContestDetail>("get_oj_manage_contest", {
      siteId: site.id,
      contestId,
    }),
  );
  const view: TaView = taView ?? "students";
  if (state.status === "idle" || (state.status === "loading" && !state.previous)) {
    return <LoadingIndicator size={24} aria-label="正在读取测验" />;
  }
  if (state.status === "error" && !state.previous) {
    return (
      <Banner status="warning">
        <Banner.Title>测验读取失败</Banner.Title>
        <Banner.Description>{state.message}</Banner.Description>
        <Button variant="tonal" onClick={() => void reload()}>
          重试
        </Button>
      </Banner>
    );
  }
  const contest = state.status === "ready" ? state.data : state.previous!;
  const now = contest.now;
  const stage = taPhase(contest, now);
  return (
    <section className="oj-ta-contest">
      <Card variant="outlined" headline={contest.title}>
        <div className="oj-ta-contest-head">
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
            <span className="oj-muted">{contest.groupName || `班级 ${contest.groupId}`}</span>
          </div>
          <p className="oj-muted">
            {new Date(contest.begin).toLocaleString()} 至 {new Date(contest.end).toLocaleString()}
            {contest.totalScore != null ? ` · 总分 ${contest.totalScore}` : ""}
          </p>
        </div>
        <div slot="actions">
          <Button
            variant="tonal"
            disabled={state.status === "loading"}
            onClick={() => void reload()}
          >
            刷新测验
          </Button>
        </div>
      </Card>
      <AdaptiveTabs
        className="oj-ta-view-tabs"
        items={[
          { value: "students", label: "学生批阅" },
          { value: "problems", label: "按题查看" },
        ]}
        value={view}
        onChange={(next) =>
          onNavigate({
            taView: next as TaView,
            review: undefined,
            problem: undefined,
            taTab: undefined,
            codeSubmission: undefined,
          })
        }
        label="助教视图"
        ariaLabel="助教测验视图"
      />
      {state.status === "error" && state.previous && (
        <Banner status="warning">
          <Banner.Description>刷新失败，正在显示上次读取的内容：{state.message}</Banner.Description>
        </Banner>
      )}
      {view === "students" ? (
        review ? (
          <TaReviewDetail
            key={review}
            site={site}
            contestId={contestId}
            submissionId={review}
            desktop={desktop}
          />
        ) : (
          <TaReviewList
            site={site}
            contestId={contestId}
            onOpenReview={(submissionId) =>
              onNavigate({
                taView: "students",
                review: submissionId,
                problem: undefined,
                taTab: undefined,
                codeSubmission: undefined,
              })
            }
          />
        )
      ) : (
        <TaProblemView
          site={site}
          contest={contest}
          problem={problem}
          taTab={taTab}
          codeSubmission={codeSubmission}
          desktop={desktop}
          onSelectProblem={(problemKey) =>
            onNavigate({
              taView: "problems",
              problem: problemKey,
              taTab: undefined,
              review: undefined,
              codeSubmission: undefined,
            })
          }
          onSelectTab={(next) =>
            onNavigate({
              taView: "problems",
              problem,
              taTab: next,
              codeSubmission: next === "judge" ? codeSubmission : undefined,
            })
          }
          onOpenSubmission={(submissionId) =>
            onNavigate({
              taView: "problems",
              problem,
              taTab: "judge",
              codeSubmission: submissionId,
            })
          }
          onOpenStudent={onOpenStudent}
        />
      )}
    </section>
  );
}
