import { Button, Card } from "material/react";
import { AdaptiveTabs, Chip } from "../../ui";
import { Statement } from "../OjPaper";
import {
  kindName,
  problemKey,
  type ManagedContestDetail,
  type Paper,
  type Problem,
  type Site,
} from "../types";
import type { TaTab } from "./ta-route";
import { TaSubmissionDetail } from "./TaSubmissionDetail";
import { TaSubmissionTable } from "./TaSubmissionTable";

const tabs: { value: TaTab; label: string }[] = [
  { value: "students", label: "学生提交" },
  { value: "statement", label: "题面" },
  { value: "judge", label: "判题详情" },
  { value: "answer", label: "作答" },
];

export function TaProblemView({
  site,
  contest,
  problem,
  taTab,
  codeSubmission,
  desktop,
  onSelectProblem,
  onSelectTab,
  onOpenSubmission,
  onOpenStudent,
}: {
  site: Site;
  contest: ManagedContestDetail;
  problem?: string;
  taTab?: TaTab;
  codeSubmission?: string;
  desktop: boolean;
  onSelectProblem: (problemKey: string) => void;
  onSelectTab: (tab: TaTab) => void;
  onOpenSubmission: (submissionId: string) => void;
  onOpenStudent: (problemKey: string) => void;
}) {
  const selected = contest.problems.find((p) => problemKey(p) === problem);
  if (!selected) {
    return <ProblemPicker contest={contest} onSelectProblem={onSelectProblem} />;
  }
  const paper: Paper = {
    contest: {
      id: contest.id,
      groupId: contest.groupId,
      title: contest.title,
      description: contest.description,
      begin: contest.begin,
      end: contest.end,
      now: contest.now,
      isExam: contest.isExam,
    },
    problems: contest.problems,
    drafts: {},
    account: site.username,
    epoch: site.sessionGeneration ?? 0,
    writable: false,
    submitted: null,
    sourceUrl: `${site.url}#/contest/${contest.id}`,
  };
  const current = taTab ?? (selected.kind === 0 || selected.kind === 4 ? "students" : "statement");
  const programming = selected.kind === 0 || selected.kind === 4;
  return (
    <section className="oj-ta-problem">
      <div className="oj-actions oj-ta-problem-head">
        <Button variant="text" onClick={() => onSelectProblem("")}>
          全部题目
        </Button>
        <h2>
          {selected.displayLabel}. {selected.title}
        </h2>
        <Chip>{kindName[selected.kind]}</Chip>
        {selected.score != null && <Chip>{selected.score} 分</Chip>}
      </div>
      <AdaptiveTabs
        items={tabs}
        value={current}
        onChange={(next) => onSelectTab(next as TaTab)}
        label="题目详情"
        ariaLabel="助教题目详情"
      />
      {current === "students" &&
        (programming ? (
          <TaSubmissionTable
            site={site}
            contestId={contest.id}
            problem={selected}
            onOpenSubmission={onOpenSubmission}
          />
        ) : (
          <Card variant="outlined">
            <p className="p-4 oj-muted">
              该题型没有编程提交；请在「学生批阅」中查看学生答案与评分。
            </p>
          </Card>
        ))}
      {current === "statement" && <Statement p={selected} paper={paper} siteId={site.id} />}
      {current === "judge" &&
        (codeSubmission ? (
          <TaSubmissionDetail
            key={`${selected.kind}:${selected.id}:${codeSubmission}`}
            site={site}
            contestId={contest.id}
            problem={selected}
            submissionId={codeSubmission}
            desktop={desktop}
          />
        ) : (
          <Card variant="outlined">
            <p className="p-4 oj-muted">
              请先在「学生提交」中选择一份提交，再查看它的测试点、隐藏输出与错误点。
            </p>
          </Card>
        ))}
      {current === "answer" && (
        <Card variant="outlined">
          <div className="p-4 space-y-2">
            <h3>助教模式下不内嵌作答</h3>
            <p className="oj-muted">
              助教通常只需要批阅与查看提交。如需自己作答，将打开「我的班级」中的同一题目。
            </p>
            <Button variant="tonal" onClick={() => onOpenStudent(problemKey(selected))}>
              打开学生作答页
            </Button>
          </div>
        </Card>
      )}
    </section>
  );
}

function ProblemPicker({
  contest,
  onSelectProblem,
}: {
  contest: ManagedContestDetail;
  onSelectProblem: (problemKey: string) => void;
}) {
  const groups = new Map<number, Problem[]>();
  for (const problem of contest.problems) {
    const rows = groups.get(problem.kind) ?? [];
    rows.push(problem);
    groups.set(problem.kind, rows);
  }
  return (
    <section className="oj-ta-problems">
      {[...groups.entries()].map(([kind, rows]) => (
        <div key={kind} className="oj-ta-problem-group">
          <h3>{kindName[kind] ?? `题型 ${kind}`}</h3>
          <div className="oj-site-grid">
            {rows.map((problem) => (
              <Card
                key={problemKey(problem)}
                variant="outlined"
                headline={`${problem.displayLabel}. ${problem.title}`}
                subhead={problem.score != null ? `${problem.score} 分` : undefined}
              >
                <div slot="actions">
                  <Button variant="tonal" onClick={() => onSelectProblem(problemKey(problem))}>
                    查看
                  </Button>
                </div>
              </Card>
            ))}
          </div>
        </div>
      ))}
      {contest.problems.length === 0 && <p className="oj-muted">该测验没有可见题目。</p>}
    </section>
  );
}
