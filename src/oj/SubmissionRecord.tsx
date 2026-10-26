import { lazy, Suspense, useState } from "react";
import { Card, LoadingIndicator } from "material/react";
import { Chip, Disclosure } from "../ui";
import { CodeEditor } from "./CodeEditor";
import { JudgeResult, Verdict } from "./JudgeResult";
import { submissionDate } from "./judging";
import { languageName, type Submission } from "./types";
const MonacoEditor = lazy(() => import("./MonacoEditor"));
const unchanged = () => {};

export function SubmissionRecord({
  submission,
  desktop,
  polling,
  error,
  onShow,
}: {
  submission: Submission;
  desktop: boolean;
  polling: boolean;
  error?: string;
  onShow: () => void;
}) {
  const [expanded, setExpanded] = useState(new Set<string>());
  const label = `提交 ${submission.id} 代码`;
  return (
    <Card variant="outlined">
      <div className="p-4 space-y-2">
        <div className="oj-actions">
          <strong>#{submission.id}</strong>
          <Verdict status={submission.status} empty="已保存答卷" />
          {submission.language && (
            <Chip variant="primary" color="accent" aria-label="代码语言">
              {languageName[submission.language] ?? submission.language}
            </Chip>
          )}
        </div>
        <p className="text-sm text-muted">{submissionDate(submission)}</p>
        <div className="space-y-2 mt-2">
          {submission.language && (
            <Disclosure
              title="查看代码"
              open={expanded.has("code")}
              onOpenChange={(next) => {
                const s = new Set(expanded);
                if (next) {
                  s.add("code");
                  if (submission.code === undefined) onShow();
                } else {
                  s.delete("code");
                }
                setExpanded(s);
              }}
            >
              <div className="pt-2">
                {expanded.has("code") &&
                  (submission.code !== undefined ? (
                    desktop ? (
                      <Suspense
                        fallback={<LoadingIndicator size={24} aria-label="加载只读编辑器" />}
                      >
                        <MonacoEditor
                          value={submission.code}
                          language={submission.language}
                          disabled
                          onChange={unchanged}
                          ariaLabel={label}
                        />
                      </Suspense>
                    ) : (
                      <CodeEditor
                        value={submission.code}
                        language={submission.language}
                        disabled
                        onChange={unchanged}
                        ariaLabel={label}
                      />
                    )
                  ) : (
                    <p>{polling ? "正在加载代码…" : (error ?? "服务器未提供提交代码。")}</p>
                  ))}
              </div>
            </Disclosure>
          )}
          <Disclosure
            title="查看结果"
            open={expanded.has("result")}
            onOpenChange={(next) => {
              const s = new Set(expanded);
              if (next) {
                s.add("result");
                onShow();
              } else {
                s.delete("result");
              }
              setExpanded(s);
            }}
          >
            <div className="pt-2">
              {expanded.has("result") && (
                <JudgeResult
                  submission={submission}
                  polling={polling}
                  error={error}
                  onRefresh={onShow}
                />
              )}
            </div>
          </Disclosure>
        </div>
      </div>
    </Card>
  );
}

export function ResultAccordion(props: {
  submission: Submission;
  polling: boolean;
  error?: string;
  onRefresh: () => void;
}) {
  return (
    <Disclosure
      defaultOpen
      title={
        <span className="flex items-center gap-2">
          <span>判题结果 #{props.submission.id}</span>
          <Verdict status={props.submission.status} empty="查询中" />
        </span>
      }
    >
      <div className="pt-2">
        <JudgeResult {...props} />
      </div>
    </Disclosure>
  );
}
