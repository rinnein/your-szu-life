import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, LoadingIndicator, TextField } from "material/react";
import { Banner, Chip, Table, bindField } from "../../ui";
import { reviewTag, reviewTagColor, type ReviewList, type Site } from "../types";
import { taReviewsKey } from "./ta-cache";
import { useTaList } from "./useTaList";

const LIMIT = 20;

export function TaReviewList({
  site,
  contestId,
  onOpenReview,
}: {
  site: Site;
  contestId: string;
  onOpenReview: (submissionId: string) => void;
}) {
  const [input, setInput] = useState("");
  const [search, setSearch] = useState("");
  const [offset, setOffset] = useState(0);
  const cacheKey = taReviewsKey(
    site.id,
    site.username,
    site.sessionGeneration,
    contestId,
    `${search}:${offset}:${LIMIT}`,
  );
  const { state, refresh } = useTaList<ReviewList>({
    cacheKey,
    load: () =>
      invoke<ReviewList>("get_oj_review_list", {
        siteId: site.id,
        contestId,
        search: search || null,
        offset,
        limit: LIMIT,
      }),
  });
  const data =
    state.status === "ready"
      ? state.data.data
      : state.status === "loading" || state.status === "error"
        ? state.previous?.data
        : undefined;
  const updatedAt =
    state.status === "ready"
      ? state.data.updatedAt
      : state.status === "loading" || state.status === "error"
        ? state.previous?.updatedAt
        : undefined;
  const submitSearch = () => {
    setOffset(0);
    setSearch(input.trim());
  };
  return (
    <section className="oj-ta-review-list">
      <div className="oj-actions oj-ta-toolbar">
        <TextField
          label="搜索学生"
          value={input}
          {...bindField(setInput)}
          placeholder="用户名或姓名"
        />
        <Button variant="tonal" onClick={submitSearch}>
          查询
        </Button>
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
          <Banner.Title>学生列表读取失败</Banner.Title>
          <Banner.Description>{state.message}</Banner.Description>
        </Banner>
      )}
      {(state.status === "loading" || state.status === "idle") && !data && (
        <LoadingIndicator size={24} aria-label="正在读取学生列表" />
      )}
      {updatedAt && (
        <p className="oj-muted">
          列表更新于 {new Date(updatedAt).toLocaleString()}
          {state.status === "loading" ? " · 正在刷新" : ""}
        </p>
      )}
      {data && data.rows.length === 0 && <p className="oj-muted">暂无学生答卷。</p>}
      {data && data.rows.length > 0 && (
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="学生答卷列表" className="min-w-[520px]">
              <Table.Header>
                <Table.Column isRowHeader>学生</Table.Column>
                <Table.Column>总分</Table.Column>
                <Table.Column>状态</Table.Column>
                <Table.Column>操作</Table.Column>
              </Table.Header>
              <Table.Body>
                {data.rows.map((row) => (
                  <Table.Row key={row.id}>
                    <Table.Cell>
                      <strong>{row.user.username}</strong>
                      {row.user.name && <span className="oj-muted"> · {row.user.name}</span>}
                    </Table.Cell>
                    <Table.Cell>{row.totalScore}</Table.Cell>
                    <Table.Cell>
                      <Chip color={reviewTagColor(row)}>{reviewTag(row)}</Chip>
                    </Table.Cell>
                    <Table.Cell>
                      <Button variant="tonal" onClick={() => onOpenReview(row.id)}>
                        批阅
                      </Button>
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
