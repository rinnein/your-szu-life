import { Alert, Button, Card, Chip, Label, ProgressBar, Table } from "@heroui/react";
import { LuCheck, LuClock, LuCircleHelp, LuX } from "react-icons/lu";
import { judgeDetails, megabytes, milliseconds, submissionDate, verdicts } from "./judging";
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
      <Chip.Label>{verdict?.code ?? label}</Chip.Label>
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
  const details = judgeDetails(submission);
  const passed = details.cases.filter((c) => c.status === 0).length;
  return (
    <Card aria-label={`提交 ${submission.id} 判题结果`}>
      <Card.Header className="flex flex-wrap items-center justify-between gap-3">
        <Card.Title>提交 #{submission.id}</Card.Title>
        <Verdict status={submission.status} empty={polling ? "查询中" : "已提交"} />
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        <p className="text-sm text-muted">
          {submissionDate(submission)}{" "}
          {languageName[submission.language ?? ""] ?? submission.language}
        </p>
        {polling && (
          <ProgressBar isIndeterminate aria-label="正在获取判题结果">
            <Label>正在获取判题结果…</Label>
            <ProgressBar.Track>
              <ProgressBar.Fill />
            </ProgressBar.Track>
          </ProgressBar>
        )}
        {error && (
          <Alert status="warning">
            <Alert.Content>{error}</Alert.Content>
          </Alert>
        )}
        <div className="grid grid-cols-2 gap-3">
          <Card variant="secondary">
            <Card.Content>
              <p className="text-sm text-muted">最大耗时</p>
              <strong>{milliseconds(details.cpu)}</strong>
            </Card.Content>
          </Card>
          <Card variant="secondary">
            <Card.Content>
              <p className="text-sm text-muted">峰值内存</p>
              <strong>{megabytes(details.memory)}</strong>
            </Card.Content>
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
            <ProgressBar
              aria-label="测试点通过比例"
              value={passed}
              maxValue={details.cases.length}
              color={passed === details.cases.length ? "success" : "warning"}
            >
              <Label>
                通过 {passed} / {details.cases.length} 个可见测试点
              </Label>
              <ProgressBar.Track>
                <ProgressBar.Fill />
              </ProgressBar.Track>
            </ProgressBar>
            <Table>
              <Table.ScrollContainer>
                <Table.Content aria-label="测试点判题结果" className="min-w-[460px]">
                  <Table.Header>
                    <Table.Column isRowHeader>测试点</Table.Column>
                    <Table.Column>结果</Table.Column>
                    <Table.Column>耗时</Table.Column>
                    <Table.Column>实际耗时</Table.Column>
                    <Table.Column>内存</Table.Column>
                  </Table.Header>
                  <Table.Body>
                    {details.cases.map((row, index) => (
                      <Table.Row key={`${row.id}:${index}`}>
                        <Table.Cell>{row.id}</Table.Cell>
                        <Table.Cell>
                          <Verdict status={row.status} empty="未提供" />
                        </Table.Cell>
                        <Table.Cell>{milliseconds(row.cpu)}</Table.Cell>
                        <Table.Cell>{milliseconds(row.real)}</Table.Cell>
                        <Table.Cell>{megabytes(row.memory)}</Table.Cell>
                      </Table.Row>
                    ))}
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
        <Button variant="secondary" isDisabled={polling} onPress={onRefresh}>
          刷新判题
        </Button>
      </Card.Content>
    </Card>
  );
}
