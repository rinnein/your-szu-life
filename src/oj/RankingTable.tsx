import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Chip, Modal, Spinner, Table } from "@heroui/react";
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
        <Button variant="secondary" isDisabled={loading} onPress={refresh}>
          刷新排名
        </Button>
      </div>
      {error && (
        <Alert status="warning">
          <Alert.Content>{error}</Alert.Content>
        </Alert>
      )}
      {loading && <Spinner aria-label="加载测验排名" />}
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
              variant="secondary"
              isDisabled={offset === 0 || loading}
              onPress={() => setOffset((v) => Math.max(0, v - 20))}
            >
              上一页
            </Button>
            <span>
              {Math.floor(offset / 20) + 1} / {Math.max(1, Math.ceil(data.count / 20))}
            </span>
            <Button
              variant="secondary"
              isDisabled={offset + 20 >= data.count || loading}
              onPress={() => setOffset((v) => v + 20)}
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
      <Button variant="secondary" onPress={() => setOpen(true)}>
        排名
      </Button>
      <Modal isOpen={open} onOpenChange={setOpen}>
        <Modal.Backdrop>
          <Modal.Container size="cover">
            <Modal.Dialog>
              <Modal.CloseTrigger />
              <Modal.Header>
                <Modal.Heading>排名</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
                <RankingContent siteId={siteId} paper={paper} />
              </Modal.Body>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
    </>
  );
}
