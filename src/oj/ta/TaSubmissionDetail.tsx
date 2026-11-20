import { invoke } from "@tauri-apps/api/core";
import { Button, LoadingIndicator } from "material/react";
import { Banner } from "../../ui";
import { useAsyncLoad } from "../../async-state";
import { JudgeResult } from "../JudgeResult";
import type { Problem, Site, TaSubmission } from "../types";
import { TaCode } from "./TaCode";

/** 单份提交的代码、判题结果、隐藏测试点期望输出与 SPJ 输出。 */
export function TaSubmissionDetail({
  site,
  contestId,
  problem,
  submissionId,
  desktop,
}: {
  site: Site;
  contestId: string;
  problem: Pick<Problem, "id" | "kind" | "displayLabel">;
  submissionId: string;
  desktop: boolean;
}) {
  const { state, reload } = useAsyncLoad(() =>
    invoke<TaSubmission>("get_oj_ta_submission", {
      siteId: site.id,
      contestId,
      problemId: problem.id,
      kind: problem.kind,
      submissionId,
    }),
  );
  if (state.status === "idle" || (state.status === "loading" && !state.previous)) {
    return <LoadingIndicator size={24} aria-label="正在读取提交详情" />;
  }
  if (state.status === "error" && !state.previous) {
    return (
      <Banner status="warning">
        <Banner.Title>提交详情读取失败</Banner.Title>
        <Banner.Description>{state.message}</Banner.Description>
        <Button variant="tonal" onClick={() => void reload()}>
          重试
        </Button>
      </Banner>
    );
  }
  const data = state.status === "ready" ? state.data : state.previous!;
  const submission = data.submission;
  return (
    <section className="oj-ta-submission">
      {state.status === "error" && state.previous && (
        <Banner status="warning">
          <Banner.Description>刷新失败，正在显示上次读取的内容：{state.message}</Banner.Description>
        </Banner>
      )}
      {data.warnings.length > 0 && (
        <Banner status="info">
          <Banner.Description>{data.warnings.join("；")}</Banner.Description>
        </Banner>
      )}
      {submission.code != null && (
        <div className="oj-ta-code">
          <TaCode
            value={submission.code}
            language={submission.language}
            desktop={desktop}
            ariaLabel={`提交 ${submissionId} 代码`}
          />
        </div>
      )}
      <JudgeResult
        submission={submission}
        polling={false}
        onRefresh={() => void reload()}
        expected={data.expected}
        showExpected
      />
    </section>
  );
}
