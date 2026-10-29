import { Fragment, useState } from "react";
import { Card, Progress, Button } from "material/react";
import { Banner, Chip, Table } from "../ui";
import { LuCheck, LuClock, LuCircleHelp, LuX, LuChevronDown, LuChevronRight } from "react-icons/lu";
import { judgeDetails, megabytes, milliseconds, submissionDate, verdicts } from "./judging";
import { SampleField } from "./SampleField";
import { languageName, type Submission } from "./types";

export function Verdict({ status, empty = "未提交" }: { status?: number; empty?: string }) {
  const verdict = status === undefined ? undefined : verdicts[status];
  const Icon =
    status === 0 ? LuCheck : status === 6 || status === 7 ? LuClock : verdict ? LuX : LuCircleHelp;
  const label = verdict
    ? `${verdict.code} · ${verdict.label}`
    : status === undefined
      ? empty
      : `未知状态 ${status}`;
  return (
    <Chip variant="primary" color={verdict?.color ?? "default"} size="sm" aria-label={label}>
      <Icon aria-hidden="true" />
      {verdict?.code ?? label}
    </Chip>
  );
}

export function JudgeResult({
  submission,
  polling,
  error,
  onRefresh,
}: {
  submission: Submission;
  polling: boolean;
  error?: string;
  onRefresh: () => void;
}) {
  const [expandedCases, setExpandedCases] = useState<Set<string>>(new Set());
  const toggleCase = (caseId: string) => {
    setExpandedCases((prev) => {
      const next = new Set(prev);
      if (next.has(caseId)) {
        next.delete(caseId);
      } else {
        next.add(caseId);
      }
      return next;
    });
  };
  const details = judgeDetails(submission);
  const passed = details.cases.filter((c) => c.status === 0).length;
  return (
    <Card aria-label={`提交 ${submission.id} 判题结果`} variant="outlined">
      <div slot="headline" className="flex flex-wrap items-center justify-between gap-3 w-full">
        <span className="font-semibold text-base">提交 #{submission.id}</span>
        <Verdict status={submission.status} empty={polling ? "查询中" : "已提交"} />
      </div>
      <div className="flex flex-col gap-4 p-4">
        <p className="text-sm text-muted">
          {submissionDate(submission)}{" "}
          {languageName[submission.language ?? ""] ?? submission.language}
        </p>
        {polling && (
          <div className="space-y-1">
            <span className="text-xs text-muted">正在获取判题结果…</span>
            <Progress variant="linear" indeterminate aria-label="正在获取判题结果" />
          </div>
        )}
        {error && <Banner status="warning">{error}</Banner>}
        <div className="grid grid-cols-2 gap-3">
          <Card variant="filled">
            <div className="p-3">
              <p className="text-sm text-muted">最大耗时</p>
              <strong>{milliseconds(details.cpu)}</strong>
            </div>
          </Card>
          <Card variant="filled">
            <div className="p-3">
              <p className="text-sm text-muted">峰值内存</p>
              <strong>{megabytes(details.memory)}</strong>
            </div>
          </Card>
        </div>
        {details.message && (
          <section>
            <h3>{submission.status === -2 ? "编译信息" : "判题信息"}</h3>
            <pre>{details.message}</pre>
          </section>
        )}
        {!!details.cases.length && (
          <>
            <div className="space-y-1.5">
              <span className="text-xs text-muted">
                通过 {passed} / {details.cases.length} 个可见测试点
              </span>
              <Progress
                variant="linear"
                aria-label="测试点通过比例"
                value={passed}
                max={details.cases.length}
              />
            </div>
            <Table>
              <Table.ScrollContainer>
                <Table.Content aria-label="测试点判题结果" className="min-w-[460px]">
                  <Table.Header>
                    <Table.Column className="w-10" aria-label="展开" />
                    <Table.Column isRowHeader>测试点</Table.Column>
                    <Table.Column>结果</Table.Column>
                    <Table.Column>耗时</Table.Column>
                    <Table.Column>实际耗时</Table.Column>
                    <Table.Column>内存</Table.Column>
                  </Table.Header>
                  <Table.Body>
                    {details.cases.map((row, index) => {
                      const isExpanded = expandedCases.has(row.id);
                      return (
                        <Fragment key={`${row.id}:${index}`}>
                          <Table.Row
                            onClick={() => toggleCase(row.id)}
                            className="cursor-pointer hover:bg-surface-container-high transition-colors"
                            aria-expanded={isExpanded}
                          >
                            <Table.Cell className="w-10 text-center">
                              <button
                                type="button"
                                className="inline-flex items-center justify-center p-1 rounded hover:bg-surface-container-highest transition-transform"
                                aria-label={
                                  isExpanded
                                    ? `收起测试点 ${row.id} 详情`
                                    : `展开测试点 ${row.id} 详情`
                                }
                                onClick={(e) => {
                                  e.stopPropagation();
                                  toggleCase(row.id);
                                }}
                              >
                                {isExpanded ? (
                                  <LuChevronDown
                                    className="w-4 h-4 text-muted"
                                    aria-hidden="true"
                                  />
                                ) : (
                                  <LuChevronRight
                                    className="w-4 h-4 text-muted"
                                    aria-hidden="true"
                                  />
                                )}
                              </button>
                            </Table.Cell>
                            <Table.Cell>{row.id}</Table.Cell>
                            <Table.Cell>
                              <Verdict status={row.status} empty="未提供" />
                            </Table.Cell>
                            <Table.Cell>{milliseconds(row.cpu)}</Table.Cell>
                            <Table.Cell>{milliseconds(row.real)}</Table.Cell>
                            <Table.Cell>{megabytes(row.memory)}</Table.Cell>
                          </Table.Row>
                          {isExpanded && (
                            <Table.Row
                              key={`${row.id}:${index}:detail`}
                              className="bg-surface-container-lowest/60 hover:bg-surface-container-lowest/60"
                            >
                              <Table.Cell colSpan={6} isRowHeader={false} className="p-3">
                                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                                  <SampleField
                                    label={`测试点 ${row.id} 输入 (stdin)`}
                                    ariaLabel={`测试点 ${row.id} 输入`}
                                    value={row.input}
                                    placeholder="（服务端未返回测试输入）"
                                  />
                                  <SampleField
                                    label={`测试点 ${row.id} 在线输出 (stdout)`}
                                    ariaLabel={`测试点 ${row.id} 输出`}
                                    value={row.output}
                                    placeholder="（服务端未返回标准输出）"
                                  />
                                </div>
                              </Table.Cell>
                            </Table.Row>
                          )}
                        </Fragment>
                      );
                    })}
                  </Table.Body>
                </Table.Content>
              </Table.ScrollContainer>
            </Table>
          </>
        )}
        {!polling && !details.cases.length && !details.message && (
          <p className="text-sm text-muted">服务器未提供测试点明细。</p>
        )}
        {details.lintScore !== undefined && <p>代码规范分：{details.lintScore}</p>}
        {details.lintMessage && <pre>{details.lintMessage}</pre>}
        <Button variant="tonal" disabled={polling} onClick={onRefresh}>
          刷新判题
        </Button>
      </div>
    </Card>
  );
}
