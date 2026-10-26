import { Disclosure, Table } from "../ui";
import type { CourseProgress, ProgressMetric } from "./types";

export const formatNumber = (value: number | null | undefined) =>
  value == null ? "—" : new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(value);
export const formatPercent = (value: number | null | undefined) =>
  value == null ? "—" : `${formatNumber(value)}%`;

export function CourseProgressDetails({
  data,
  title,
  onRefresh,
}: {
  data?: CourseProgress | null;
  title: string;
  onRefresh?: () => void;
}) {
  const metrics: Array<{ id: string; name: string; metric?: ProgressMetric }> = [
    { id: "signin", name: "签到", metric: data?.signin },
    { id: "video", name: "视频", metric: data?.video },
    { id: "homework", name: "作业", metric: data?.homework },
    { id: "test", name: "测验", metric: data?.test },
    { id: "discussion", name: "讨论", metric: data?.discussion },
    { id: "exam", name: "考试", metric: data?.exam },
  ];
  return (
    <Disclosure
      title="课程详情 · 权重与任务"
      ariaLabel={`${title}的课程详情`}
      onOpenChange={(open) => {
        if (open) onRefresh?.();
      }}
    >
      <div className="grid gap-3 pt-3">
        <div className="grid gap-1 text-sm text-muted sm:grid-cols-3">
          <span>平台总进度：{formatPercent(data?.totalPercent)}</span>
          <span>当前成绩：{formatNumber(data?.score)}</span>
          <span>加权成绩：{formatNumber(data?.weightedScore)}</span>
        </div>
        <div className="w-full min-w-0 max-w-full overflow-hidden">
          <p className="text-[11px] text-muted mb-1.5 sm:hidden">
            左右滑动可查看完整考核权重与完成明细
          </p>
          <Table>
            <Table.ScrollContainer className="w-full min-w-0 max-w-full overflow-x-auto">
              <Table.Content aria-label={`${title}的考核权重与任务`} className="min-w-[400px]">
                <Table.Header>
                  <Table.Column id="category" isRowHeader>
                    任务类别
                  </Table.Column>
                  <Table.Column id="weight">权重占比</Table.Column>
                  <Table.Column id="progress">完成率</Table.Column>
                  <Table.Column id="tasks">完成 / 总数</Table.Column>
                  <Table.Column id="score">成绩贡献</Table.Column>
                </Table.Header>
                <Table.Body>
                  {metrics.map(({ id, name, metric }) => {
                    const weight =
                      metric?.weight ?? (id === "signin" ? data?.signinWeight : undefined);
                    return (
                      <Table.Row
                        key={id}
                        id={id}
                        className={weight === 0 ? "opacity-50" : undefined}
                      >
                        <Table.Cell className="whitespace-nowrap">{name}</Table.Cell>
                        <Table.Cell className="whitespace-nowrap">
                          {formatPercent(weight)}
                          {weight === 0 && (
                            <span className="block text-xs text-muted">不计入进度</span>
                          )}
                        </Table.Cell>
                        <Table.Cell className="whitespace-nowrap">
                          {formatPercent(metric?.percent)}
                        </Table.Cell>
                        <Table.Cell className="whitespace-nowrap">
                          {id === "discussion" && "参与 "}
                          {metric?.count ?? "—"} / {metric?.total ?? "—"}
                          {metric?.total === 0 && (
                            <span className="block text-xs text-muted">暂无任务</span>
                          )}
                        </Table.Cell>
                        <Table.Cell className="whitespace-nowrap">
                          {formatNumber(metric?.score)}
                          {metric?.averageScore != null && (
                            <span className="block text-xs text-muted">
                              平均分 {formatNumber(metric.averageScore)}
                            </span>
                          )}
                        </Table.Cell>
                      </Table.Row>
                    );
                  })}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
        </div>
        <div className="grid gap-1 text-xs text-muted">
          <p>
            权重来自当前课程。总进度按完成率加权，成绩单独展示；完成率缺失时按完成数与总数计算，无法确定则显示未知。
          </p>
          {data?.weightTotal != null && <p>权重合计：{formatPercent(data.weightTotal)}</p>}
          {data?.signinTime && <p>最近签到：{data.signinTime}</p>}
          {data?.videoMinutes != null && (
            <p>已记录视频时长：{formatNumber(data.videoMinutes)} 分钟</p>
          )}
          {(data?.discussionPosts != null || data?.discussionReplies != null) && (
            <p>
              讨论：发帖 {data?.discussionPosts ?? "—"} 次、回复 {data?.discussionReplies ?? "—"} 次
            </p>
          )}
        </div>
      </div>
    </Disclosure>
  );
}
