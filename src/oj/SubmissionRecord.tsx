import { lazy, Suspense, useState } from "react";
import { Accordion, Card, Chip, Spinner } from "@heroui/react";
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
    <Card>
      <Card.Content>
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
        <Accordion
          allowsMultipleExpanded
          expandedKeys={expanded}
          onExpandedChange={(keys) => {
            const next = new Set(Array.from(keys, String));
            if (
              (next.has("result") && !expanded.has("result")) ||
              (next.has("code") && !expanded.has("code") && submission.code === undefined)
            )
              onShow();
            setExpanded(next);
          }}
        >
          {submission.language && (
            <Accordion.Item id="code">
              <Accordion.Heading>
                <Accordion.Trigger>
                  查看代码
                  <Accordion.Indicator />
                </Accordion.Trigger>
              </Accordion.Heading>
              <Accordion.Panel>
                <Accordion.Body>
                  {expanded.has("code") &&
                    (submission.code !== undefined ? (
                      desktop ? (
                        <Suspense fallback={<Spinner aria-label="加载只读编辑器" />}>
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
                </Accordion.Body>
              </Accordion.Panel>
            </Accordion.Item>
          )}
          <Accordion.Item id="result">
            <Accordion.Heading>
              <Accordion.Trigger>
                查看结果
                <Accordion.Indicator />
              </Accordion.Trigger>
            </Accordion.Heading>
            <Accordion.Panel>
              <Accordion.Body>
                {expanded.has("result") && (
                  <JudgeResult
                    submission={submission}
                    polling={polling}
                    error={error}
                    onRefresh={onShow}
                  />
                )}
              </Accordion.Body>
            </Accordion.Panel>
          </Accordion.Item>
        </Accordion>
      </Card.Content>
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
    <Accordion variant="surface">
      <Accordion.Item>
        <Accordion.Heading>
          <Accordion.Trigger>
            判题结果 #{props.submission.id}
            <Verdict status={props.submission.status} empty="查询中" />
            <Accordion.Indicator />
          </Accordion.Trigger>
        </Accordion.Heading>
        <Accordion.Panel>
          <Accordion.Body>
            <JudgeResult {...props} />
          </Accordion.Body>
        </Accordion.Panel>
      </Accordion.Item>
    </Accordion>
  );
}
