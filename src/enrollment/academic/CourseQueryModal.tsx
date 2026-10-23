import { useMemo, useState } from "react";
import { Button, Chip, Input, Label, Modal, Table, TextField } from "@heroui/react";
import { Choice } from "../shared";
import { useLogicalNavigation } from "../../logical-navigation";
import {
  aggregateCourses,
  defaultFilters,
  filterCourses,
  nodePath,
  string,
  type AcademicTree,
  type Filters,
} from "./tree";
import { getViewState, saveViewState } from "./view-state";
export default function CourseQueryModal({
  tree,
  nodeId,
  viewKey,
  onClose,
}: {
  tree: AcademicTree;
  nodeId: string;
  viewKey: string;
  onClose: () => void;
}) {
  const nav = useLogicalNavigation();
  const [filters, setFilters] = useState<Filters>(
    () => getViewState(viewKey).filters[nodeId] ?? { ...defaultFilters },
  );
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [details, setDetails] = useState<string>();
  const update = (patch: Partial<Filters>) =>
    setFilters((old) => {
      const next = { ...old, ...patch, page: patch.page ?? 1 };
      saveViewState(viewKey, { filters: { ...getViewState(viewKey).filters, [nodeId]: next } });
      return next;
    });
  const rows = useMemo(() => aggregateCourses(tree, nodeId), [tree, nodeId]);
  const groups = useMemo(
    () => [...new Map(rows.flatMap((r) => r.groups.map((g) => [g.id, g] as const))).values()],
    [rows],
  );
  const filtered = useMemo(() => filterCourses(rows, filters), [rows, filters]);
  const pages = Math.max(1, Math.ceil(filtered.length / 12));
  const page = Math.min(filters.page, pages);
  const visible = filtered.slice((page - 1) * 12, page * 12);
  const title = tree.nodes.get(nodeId)?.label ?? "课程查询";
  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Container size="cover">
        <Modal.Dialog className="mx-auto w-full max-w-[1280px] min-w-0 overflow-hidden">
          <Modal.CloseTrigger aria-label="关闭课程检索" />
          <Modal.Header>
            <Modal.Heading>{title}</Modal.Heading>
            <p className="break-all break-words pr-7 text-xs sm:text-sm text-muted">
              {nodePath(tree, nodeId).join(" / ")}
            </p>
          </Modal.Header>
          <Modal.Body className="grid min-h-0 content-start gap-4 w-full min-w-0 max-w-full overflow-hidden">
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <TextField value={filters.query} onChange={(query) => update({ query })}>
                <Label>课程名称或代码</Label>
                <Input placeholder="在此分支内检索" />
              </TextField>
              <Button
                className="sm:hidden"
                variant="secondary"
                aria-expanded={filtersOpen}
                onPress={() => setFiltersOpen((v) => !v)}
              >
                {filtersOpen ? "收起筛选" : "筛选与排序"}
              </Button>
              <div className={filtersOpen ? "contents" : "hidden sm:contents"}>
                <Choice
                  label="完成状态"
                  value={filters.status}
                  onChange={(status) => update({ status })}
                  items={[
                    { id: "all", label: "全部状态" },
                    ...["已完成", "已选未完成", "未修", "未知"].map((s) => ({ id: s, label: s })),
                    { id: "conflict", label: "记录存在差异" },
                  ]}
                />
                <Choice
                  label="所属课组"
                  value={filters.group}
                  onChange={(group) => update({ group })}
                  items={[{ id: "all", label: "全部后代课组" }, ...groups]}
                />
                <Choice
                  label="排序"
                  value={filters.sort}
                  onChange={(sort) => update({ sort })}
                  items={[
                    { id: "code", label: "课程代码" },
                    { id: "name", label: "课程名称" },
                    { id: "credits", label: "学分从高到低" },
                  ]}
                />
              </div>
            </div>
            <p className="text-sm text-muted">
              共 {filtered.length} 门课程 · 汇总当前节点及全部后代，同一课程代码已合并
            </p>
            <p className="text-xs text-muted sm:hidden">左右滑动表格查看更多列</p>
            <div className="w-full min-w-0 max-w-full overflow-hidden">
              <Table>
                <Table.ScrollContainer className="w-full min-w-0 max-w-full overflow-x-auto">
                  <Table.Content aria-label={`${title}课程检索`} className="min-w-[880px]">
                    <Table.Header>
                      <Table.Column isRowHeader>课程名称 / 代码</Table.Column>
                      <Table.Column>完成状态</Table.Column>
                      <Table.Column>课程类型</Table.Column>
                      <Table.Column>学分</Table.Column>
                      <Table.Column>所属课组</Table.Column>
                      <Table.Column>操作</Table.Column>
                    </Table.Header>
                    <Table.Body renderEmptyState={() => "没有符合条件的课程"}>
                      {visible.map((c) => (
                        <Table.Row key={c.id}>
                          <Table.Cell className="max-w-[260px] whitespace-normal">
                            <p className="font-medium">{c.names.join(" / ")}</p>
                            <p className="text-xs text-muted">{c.code || "课程代码缺失"}</p>
                            {c.conflict && (
                              <Chip size="sm" variant="soft" color="warning">
                                记录存在差异
                              </Chip>
                            )}
                            {details === c.id && (
                              <div className="mt-2 text-xs text-muted">
                                {c.records.map((r, i) => (
                                  <p key={i}>
                                    成绩：{string(r.CJ ?? r.ZSCJ) || "未知"}；替代关系：
                                    {string(r.TDKC) || "无记录"}；备注：{string(r.BZ) || "无"}
                                  </p>
                                ))}
                              </div>
                            )}
                          </Table.Cell>
                          <Table.Cell>{c.statuses.join(" / ")}</Table.Cell>
                          <Table.Cell className="max-w-40 whitespace-normal">
                            {c.types.join(" / ")}
                          </Table.Cell>
                          <Table.Cell>{c.credits.join(" / ")}</Table.Cell>
                          <Table.Cell className="max-w-52 whitespace-normal">
                            {c.groups.map((g) => g.label).join("、")}
                          </Table.Cell>
                          <Table.Cell>
                            <div className="flex gap-1">
                              <Button
                                size="sm"
                                variant="tertiary"
                                onPress={() => setDetails(details === c.id ? undefined : c.id)}
                              >
                                详情
                              </Button>
                              <Button
                                size="sm"
                                variant="secondary"
                                isDisabled={!c.code}
                                onPress={() =>
                                  void nav.push(`/enrollment?keyword=${encodeURIComponent(c.code)}`)
                                }
                              >
                                查找开课
                              </Button>
                            </div>
                          </Table.Cell>
                        </Table.Row>
                      ))}
                    </Table.Body>
                  </Table.Content>
                </Table.ScrollContainer>
              </Table>
            </div>
          </Modal.Body>
          <Modal.Footer className="flex flex-wrap items-center justify-between gap-3">
            <Button variant="tertiary" onPress={() => update({ ...defaultFilters })}>
              重置筛选
            </Button>
            <div className="flex items-center gap-3">
              <Button
                variant="secondary"
                size="sm"
                isDisabled={page === 1}
                onPress={() => update({ page: page - 1 })}
              >
                上一页
              </Button>
              <span className="text-sm">
                {page} / {pages}
              </span>
              <Button
                variant="secondary"
                size="sm"
                isDisabled={page === pages}
                onPress={() => update({ page: page + 1 })}
              >
                下一页
              </Button>
            </div>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
