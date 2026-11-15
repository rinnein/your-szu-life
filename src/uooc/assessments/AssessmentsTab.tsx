import { useMemo } from "react";
import { AdaptiveTabs } from "../../ui";
import type { UoocSnapshot, QuizState } from "../state/types";
import { quizStageStatus } from "../state/status";
import { useUoocAssessments } from "../state/use-assessments";
import { QuizListPanel } from "./QuizListPanel";
import { HomeworkListPanel } from "./HomeworkListPanel";
import { ExamListPanel } from "./ExamListPanel";

export type AssessSubtab = "quiz" | "homework" | "exam";

interface AssessmentsTabProps {
  snapshot: UoocSnapshot;
  ready: boolean;
  subtab?: AssessSubtab;
  onSubtabChange: (next: AssessSubtab) => void;
  openQuiz: (quiz: QuizState) => void;
  onSnapshot: (snapshot: UoocSnapshot) => void;
}

/**
 * 考核 tab：测验 / 作业 / 考试三个子 tab，徽标给出各自待办数。
 * 子 tab 属于同一逻辑页面的内部切换（replace），不新增返回层级。
 */
export function AssessmentsTab({
  snapshot,
  ready,
  subtab,
  onSubtabChange,
  openQuiz,
  onSnapshot,
}: AssessmentsTabProps) {
  const { exams, homeworks } = useUoocAssessments();

  const quizzes = Object.values(snapshot.quizzes ?? {}).filter(
    (q) => !snapshot.user || q.accountId === snapshot.user.id,
  );
  const quizPending = quizzes.filter(
    (q) => q.stage !== "passed" && q.stage !== "disabled" && q.stage !== "idle",
  ).length;
  const homeworkPending = homeworks.filter((hw) => hw.canDo && !(hw.submitCount >= hw.chance)).length;
  const examPending = exams.filter((exam) => exam.canDo && !(exam.submitCount >= exam.chance)).length;

  const items = useMemo(
    () => [
      { value: "quiz", label: "测验", badge: quizPending || undefined },
      { value: "homework", label: "作业", badge: homeworkPending || undefined },
      { value: "exam", label: "考试", badge: examPending || undefined },
    ],
    [quizPending, homeworkPending, examPending],
  );

  const active: AssessSubtab = subtab ?? "quiz";

  return (
    <div className="grid gap-4">
      <AdaptiveTabs
        label="考核子分类"
        items={items}
        value={active}
        onChange={(next) => onSubtabChange(next as AssessSubtab)}
        ariaLabel="考核子分类"
        injectDrawer={false}
      />
      {active === "quiz" && (
        <QuizListPanel
          courses={snapshot.courses}
          quizzes={snapshot.quizzes}
          onOpenQuiz={openQuiz}
          onSnapshot={onSnapshot}
          ready={ready}
        />
      )}
      {active === "homework" && <HomeworkListPanel courses={snapshot.courses} ready={ready} />}
      {active === "exam" && <ExamListPanel courses={snapshot.courses} ready={ready} />}
    </div>
  );
}

export { quizStageStatus };
