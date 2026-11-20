import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, LoadingIndicator, Select, SelectOption, TextField } from "material/react";
import { Banner, Chip, Table, bindField } from "../../ui";
import { useLatestRequest } from "../../async-state";
import { Verdict } from "../JudgeResult";
import { megabytes, milliseconds, submissionDate } from "../judging";
import {
  languageName,
  type Problem,
  type ProblemSubmissions,
  type Site,
  type Submission,
} from "../types";

const LIMIT = 20;
const statusOptions = [
  { value: "", label: "全部状态" },
  { value: "0", label: "答案正确" },
  { value: "-1", label: "答案错误" },
  { value: "-2", label: "编译错误" },
  { value: "1", label: "CPU 超时" },
  { value: "2", label: "运行超时" },
  { value: "3", label: "内存超限" },
  { value: "4", label: "运行时异常" },
  { value: "8", label: "部分正确" },
];

export function TaSubmissionTable({
  site,
  contestId,
  problem,
  onOpenSubmission,
}: {
  site: Site;
  contestId: string;
  problem: Problem;
  onOpenSubmission: (submissionId: string) => void;
}) {
  const [input, setInput] = useState("");
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [offset, setOffset] = useState(0);
  const { state, run } = useLatestRequest<ProblemSubmissions>(() =>
    invoke<ProblemSubmissions>("get_oj_problem_submissions", {
      siteId: site.id,
      contestId,
      problemId: problem.id,
      kind: problem.kind,
      search: search || null,
      status: status || null,
      offset,
      limit: LIMIT,
    }),
  );
  useEffect(() => {
    void run();
  }, [problem.id, problem.kind, search, status, offset, run]);
  const data =
    state.status === "ready"
      ? state.data
      : state.status === "loading" || state.status === "error"
        ? state.previous
        : undefined;
  const submitSearch = () => {
    setOffset(0);
    setSearch(input.trim());
  };
  return (
    <section className="oj-ta-submissions">
      <div className="oj-actions oj-ta-toolbar">
        <TextField
          label="搜索学生"
          value={input}
          {...bindField(setInput)}
          placeholder="用户名或姓名"
        />
        <Select
          label="判题状态"
          aria-label="判题状态"
          value={status}
          onChange={(event: any) => {
            setOffset(0);
            setStatus(String(event.detail?.value ?? event.target?.value ?? ""));
          }}
        >
          {statusOptions.map((option) => (
            <SelectOption key={option.value} value={option.value}>
              {option.label}
            </SelectOption>
          ))}
        </Select>
        <Button variant="tonal" onClick={submitSearch}>
          查询
        </Button>
        <Button variant="tonal" disabled={state.status === "loading"} onClick={() => void run()}>
          刷新
        </Button>
      </div>
      {state.status === "error" && !data && (
        <Banner status="warning">
          <Banner.Title>提交列表读取失败</Banner.Title>
          <Banner.Description>{state.message}</Banner.Description>
        </Banner>
      )}
      {state.status === "loading" && !data && (
        <LoadingIndicator size={24} aria-label="正在读取提交列表" />
      )}
      {state.status === "error" && state.previous && (
        <Banner status="warning">
          <Banner.Description>刷新失败，正在显示上次读取的内容：{state.message}</Banner.Description>
        </Banner>
      )}
      {data && data.rows.length === 0 && <p className="oj-muted">该题暂无学生提交。</p>}
      {data && data.rows.length > 0 && (
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="全班提交列表" className="min-w-[620px]">
              <Table.Header>
                <Table.Column isRowHeader>学生</Table.Column>
                <Table.Column>判定</Table.Column>
                <Table.Column>语言</Table.Column>
                <Table.Column>耗时</Table.Column>
                <Table.Column>内存</Table.Column>
                <Table.Column>提交时间</Table.Column>
                <Table.Column>操作</Table.Column>
              </Table.Header>
              <Table.Body>
                {data.rows.map((row) => (
                  <Table.Row key={row.id}>
                    <Table.Cell>
                      <strong>{row.user.username}</strong>
                      {row.user.name && <span className="oj-muted"> · {row.user.name}</span>}
                    </Table.Cell>
                    <Table.Cell>
                      <Verdict status={row.status ?? undefined} empty="未知" />
                    </Table.Cell>
                    <Table.Cell>
                      <Chip variant="primary" color="accent">
                        {languageName[row.language] ?? (row.language || "未知")}
                      </Chip>
                    </Table.Cell>
                    <Table.Cell>{milliseconds(row.cpuTime ?? undefined)}</Table.Cell>
                    <Table.Cell>{megabytes(row.memory ?? undefined)}</Table.Cell>
                    <Table.Cell>
                      {submissionDate({
                        id: row.id,
                        time: row.createTime as Submission["time"],
                      }) || "未知"}
                    </Table.Cell>
                    <Table.Cell>
                      {row.canViewDetail ? (
                        <Button variant="tonal" onClick={() => onOpenSubmission(row.id)}>
                          查看
                        </Button>
                      ) : (
                        <Chip>不可查看</Chip>
                      )}
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
      {data && data.count > LIMIT && (
        <div className="oj-actions oj-ta-pagination">
          <Button
            variant="text"
            disabled={offset === 0 || state.status === "loading"}
            onClick={() => setOffset(Math.max(0, offset - LIMIT))}
          >
            上一页
          </Button>
          <span className="oj-muted">
            第 {Math.floor(offset / LIMIT) + 1} / {Math.max(1, Math.ceil(data.count / LIMIT))} 页
          </span>
          <Button
            variant="text"
            disabled={offset + LIMIT >= data.count || state.status === "loading"}
            onClick={() => setOffset(offset + LIMIT)}
          >
            下一页
          </Button>
        </div>
      )}
    </section>
  );
}
