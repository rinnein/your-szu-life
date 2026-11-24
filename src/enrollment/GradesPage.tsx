import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Card } from "material/react";
import { LuRefreshCw } from "react-icons/lu";
import { PageShell, SkeletonTable, StatCard } from "../components";
import { Chip, Table } from "../ui";
import { useEnrollment } from "./use-enrollment";
import { Choice, ModuleHeader, OperationError } from "./shared";
import type { GradeItem } from "./model";

const displayNumber = (value: number | null | undefined) =>
  value == null || !Number.isFinite(value) ? "—" : String(Math.round(value * 100) / 100);
const displayGpa = (value: number | null | undefined) =>
  value == null || !Number.isFinite(value) ? "—" : value.toFixed(2);

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-inner bg-surface-secondary p-3">
      <p className="text-xs text-muted">{label}</p>
      <p className="mt-1 text-lg font-semibold tabular-nums">{value}</p>
    </div>
  );
}

function courseKey(item: GradeItem) {
  return `${item.termId}:${item.courseCode}:${item.section}:${item.courseName}`;
}

export default function GradesPage() {
  const { snapshot: s, run, pending, errors } = useEnrollment();
  const report = s?.grades ?? null;
  const loading = !!pending.grades;
  const autoFor = useRef<string | undefined>(undefined);
  const [termId, setTermId] = useState<string>();

  const load = useCallback(() => {
    void run({ type: "grades" });
  }, [run]);

  // 有缓存直接展示；没有缓存时等快照到达后再自动查询一次，避免重复请求。
  useEffect(() => {
    const student = s?.context.student;
    if (!s || report || loading || !student || autoFor.current === student) return;
    autoFor.current = student;
    load();
  }, [s, report, loading, load]);

  const terms = report?.terms ?? [];
  const active = terms.find((term) => term.termId === termId) ?? terms[0];
  useEffect(() => {
    if (active && active.termId !== termId) setTermId(active.termId);
  }, [active, termId]);

  const rows = useMemo(
    () => (report && active ? report.items.filter((item) => item.termId === active.termId) : []),
    [report, active],
  );

  return (
    <PageShell>
      <ModuleHeader
        title="成绩查询"
        description="逐学期课程成绩、学分与平均学分绩点"
        actions={
          <Button size="xs" variant="tonal" disabled={loading} onClick={load}>
            <LuRefreshCw />
            刷新
          </Button>
        }
      />

      <div className="mx-auto grid w-full max-w-[1200px] min-w-0 gap-5">
        <OperationError message={errors.grades} onRetry={load} />
        {!report ? (
          <Card variant="outlined">
            <div className="grid min-h-[380px] place-content-center justify-items-center gap-4 p-6 text-center">
              {loading ? (
                <div className="w-full" role="status" aria-label="正在加载成绩">
                  <SkeletonTable rows={5} cols={4} />
                </div>
              ) : (
                <>
                  <h2 className="text-lg font-semibold">还没有成绩数据</h2>
                  <p className="max-w-md text-sm text-muted">
                    应用会通过统一认证自动访问教务系统。如果统一认证已失效，请重新登录应用后再试；已有缓存会保留。
                  </p>
                  <Button variant="filled" onClick={load}>
                    查询成绩
                  </Button>
                </>
              )}
            </div>
          </Card>
        ) : (
          <>
            <div className="grid w-full min-w-0 gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <StatCard
                label="总 GPA"
                value={displayGpa(report.gpa)}
                color="brand"
                description="按学分加权，仅计有效成绩"
              />
              <StatCard
                label="已获学分"
                value={displayNumber(report.earnedCredits)}
                unit="分"
                description={`修读 ${displayNumber(report.selectedCredits)} 学分`}
              />
              <StatCard
                label="课程数"
                value={report.itemCount}
                unit="门"
                description="含无效与重修记录"
              />
              <StatCard
                label="学期数"
                value={report.terms.length}
                unit="个"
                description="按学期倒序排列"
              />
            </div>

            <Card variant="outlined" headline="学期详情">
              <div className="grid gap-4 p-4 pt-0">
                <div className="flex flex-wrap items-end justify-between gap-3">
                  <div className="w-64 max-w-full">
                    <Choice
                      label="学期"
                      value={active?.termId ?? ""}
                      onChange={setTermId}
                      items={terms.map((term) => ({
                        id: term.termId,
                        label: term.termName,
                      }))}
                    />
                  </div>
                  <p className="text-xs text-muted">
                    缓存更新于 {new Date(report.updatedAt).toLocaleString()}
                  </p>
                </div>
                {active && (
                  <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                    <Metric label="学期 GPA" value={displayGpa(active.averageGradePoint)} />
                    <Metric label="修读学分" value={displayNumber(active.selectedCredits)} />
                    <Metric label="取得学分" value={displayNumber(active.earnedCredits)} />
                    <Metric
                      label="通过率"
                      value={active.percent == null ? "—" : `${displayNumber(active.percent)}%`}
                    />
                  </div>
                )}
              </div>
            </Card>

            <Card variant="outlined">
              <div className="overflow-hidden">
                <Table>
                  <Table.ScrollContainer>
                    <Table.Content
                      aria-label={`${active?.termName ?? ""}成绩`}
                      className="min-w-[760px]"
                    >
                      <Table.Header>
                        <Table.Column isRowHeader>课程</Table.Column>
                        <Table.Column>性质</Table.Column>
                        <Table.Column>学分</Table.Column>
                        <Table.Column>成绩</Table.Column>
                        <Table.Column>绩点</Table.Column>
                        <Table.Column>考试日期</Table.Column>
                      </Table.Header>
                      <Table.Body renderEmptyState={() => "本学期暂无成绩"}>
                        {rows.map((item) => (
                          <Table.Row key={courseKey(item)}>
                            <Table.Cell>
                              <p>{item.courseName}</p>
                              <p className="text-xs text-muted">
                                {[item.courseCode, item.section ? `课序号 ${item.section}` : ""]
                                  .filter(Boolean)
                                  .join(" · ") || "—"}
                              </p>
                            </Table.Cell>
                            <Table.Cell>{item.nature}</Table.Cell>
                            <Table.Cell className="tabular-nums">
                              {displayNumber(item.credit)}
                            </Table.Cell>
                            <Table.Cell>
                              <span
                                className={item.valid ? "font-semibold" : "text-muted line-through"}
                              >
                                {item.score}
                              </span>
                              {!item.valid && (
                                <Chip variant="soft" size="sm" className="ml-2">
                                  无效
                                </Chip>
                              )}
                              {item.grade && item.grade !== item.score && (
                                <p className="text-xs text-muted">等级 {item.grade}</p>
                              )}
                            </Table.Cell>
                            <Table.Cell className="tabular-nums">
                              {displayNumber(item.gradePoint)}
                            </Table.Cell>
                            <Table.Cell>{item.examDate || "—"}</Table.Cell>
                          </Table.Row>
                        ))}
                      </Table.Body>
                    </Table.Content>
                  </Table.ScrollContainer>
                </Table>
              </div>
            </Card>

            <p className="text-xs text-muted">
              GPA 按平均学分绩点计算：Σ(学分 × 课程绩点) /
              Σ学分，只统计有效且有绩点的课程；无效与重修记录仍保留在表格中。
            </p>
          </>
        )}
      </div>
    </PageShell>
  );
}
