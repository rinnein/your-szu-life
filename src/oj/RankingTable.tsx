import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Dialog, LoadingIndicator, Button } from "material/react";
import { Banner, Chip, Table } from "../ui";
import { type Paper, type Ranking } from "./types";
import { formatRemaining } from "./ContestTable";
function RankingContent({ siteId, paper }: { siteId: string; paper: Paper }) {
  const [offset, setOffset] = useState(0);
  const [data, setData] = useState<Ranking>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((v) => v + 1), []);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(undefined);
    setData(undefined);
    void invoke<Ranking>("get_oj_ranking", { siteId, contestId: paper.contest.id, offset })
      .then((value) => {
        if (active) setData(value);
      })
      .catch((e) => {
        if (active) setError(String(e));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [siteId, paper.contest.id, paper.account, paper.epoch, offset, revision]);
  const problems = paper.problems.filter((p) => p.kind === 0);
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm text-muted">按服务器返回的排名顺序显示</span>
        <Button variant="tonal" disabled={loading} onClick={refresh}>
          刷新排名
        </Button>
      </div>
      {error && <Banner status="warning">{error}</Banner>}
      {loading && <LoadingIndicator size={24} aria-label="加载测验排名" />}
      {data && (
        <>
          <Table>
            <Table.ScrollContainer>
              <Table.Content aria-label="测验内排名" className="min-w-[640px]">
                <Table.Header>
                  <Table.Column isRowHeader>排名</Table.Column>
                  <Table.Column>账号／姓名</Table.Column>
                  <Table.Column>AC 题数</Table.Column>
                  <Table.Column>总用时</Table.Column>
                  {problems.map((p) => (
                    <Table.Column key={p.id}>{p.displayLabel}</Table.Column>
                  ))}
                </Table.Header>
                <Table.Body renderEmptyState={() => "暂无可见排名"}>
                  {data.rows.map((row) => (
                    <Table.Row
                      key={row.username}
                      className={row.username === paper.account ? "oj-ranking-self" : undefined}
                    >
                      <Table.Cell>{row.rank}</Table.Cell>
                      <Table.Cell>
                        <span>{row.username}</span>
                        {row.username === paper.account && (
                          <Chip size="sm" color="accent" variant="primary" className="ms-2">
                            我
                          </Chip>
                        )}
                        <p className="text-sm text-muted">{row.name || row.nickname}</p>
                      </Table.Cell>
                      <Table.Cell>{row.accepted}</Table.Cell>
                      <Table.Cell>{row.penalty || "—"}</Table.Cell>
                      {problems.map((p) => {
                        const cell = row.problems[p.id];
                        return (
                          <Table.Cell key={p.id}>
                            {cell ? (
                              <div className="flex flex-col gap-1">
                                <Chip
                                  size="sm"
                                  color={cell.accepted ? "success" : "danger"}
                                  variant="primary"
                                >
                                  {cell.accepted ? "AC" : "未通过"}
                                </Chip>
                                {cell.accepted && cell.seconds !== null && (
                                  <span>{formatRemaining(cell.seconds * 1000)}</span>
                                )}
                                {cell.errors > 0 && (
                                  <span className="text-xs text-muted">错误 {cell.errors} 次</span>
                                )}
                              </div>
                            ) : (
                              "—"
                            )}
                          </Table.Cell>
                        );
                      })}
                    </Table.Row>
                  ))}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
          <div className="flex items-center justify-end gap-3">
            <Button
              variant="tonal"
              disabled={offset === 0 || loading}
              onClick={() => setOffset((v) => Math.max(0, v - 20))}
            >
              上一页
            </Button>
            <span>
              {Math.floor(offset / 20) + 1} / {Math.max(1, Math.ceil(data.count / 20))}
            </span>
            <Button
              variant="tonal"
              disabled={offset + 20 >= data.count || loading}
              onClick={() => setOffset((v) => v + 20)}
            >
              下一页
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
export function RankingTable({ siteId, paper }: { siteId: string; paper: Paper }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="tonal" onClick={() => setOpen(true)}>
        排名
      </Button>
      {open &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog open={open} headline="排名" className="max-w-4xl" onClose={() => setOpen(false)}>
            <div className="max-h-[80vh] overflow-y-auto">
              <RankingContent siteId={siteId} paper={paper} />
            </div>
            <div slot="actions">
              <Button variant="tonal" onClick={() => setOpen(false)}>
                关闭
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
    </>
  );
}
