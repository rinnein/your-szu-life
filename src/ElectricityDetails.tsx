import { useState } from "react";
import { Card, Button } from "material/react";
import { Banner, Table } from "./ui";
import {
  Bar,
  CartesianGrid,
  ComposedChart,
  Legend,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { formatElectricity as fmt, type ElectricityResult } from "./electricity";
import { useLogicalNavigation } from "./logical-navigation";
import { StatCard } from "./components";

export function ElectricityDetails({
  result,
  rate,
  retryRecharges,
  retrying,
}: {
  result: ElectricityResult;
  rate?: number;
  retryRecharges: () => void;
  retrying: boolean;
}) {
  const [money, setMoney] = useState(false);
  const navigation = useLogicalNavigation();
  const summary = result.summary;
  const multiplier = money ? rate : 1;
  const unit = money ? "元（折算）" : "度";
  const converted = (v: number | null | undefined) =>
    v == null || multiplier == null ? null : v * multiplier;
  const readings = result.readings.map((r) => ({
    ...r,
    remaining: converted(r.remaining),
    dailyUsed: converted(r.dailyUsed),
  }));
  // Include calendar gaps explicitly so the line never connects missing days.
  const byDate = new Map(readings.map((r) => [r.date.slice(0, 10), r]));
  const chart = [];
  const start = Date.parse(`${result.beginDate}T00:00:00Z`);
  const end = Date.parse(`${result.endDate}T00:00:00Z`);
  for (let time = start; time <= end && chart.length < 31; time += 86400000) {
    const date = new Date(time).toISOString().slice(0, 10);
    chart.push(byDate.get(date) ?? { date, remaining: null, dailyUsed: null });
  }
  const metrics = [
    {
      title: "当前电量",
      value: result.currentRemaining,
      suffix: "度",
      sub:
        rate == null
          ? "折算比例加载中"
          : `折算余额 ${fmt(result.currentRemaining == null ? null : result.currentRemaining * rate)} 元`,
    },
    {
      title: "日均用电",
      value: summary?.dailyAverage,
      suffix: "度 / 天",
      sub: `周期内 ${summary?.validDays ?? 0} 天有效数据`,
    },
    {
      title: "预计可用天数",
      value: summary?.estimatedDaysLeft,
      suffix: "天",
      sub: "按最近 3 天有效用电估算",
    },
    { title: "周期用电", value: summary?.totalUsed, suffix: "度", sub: "最近 31 天有效用量合计" },
  ];
  return (
    <>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2" aria-label="电费概览">
        {metrics.map((metric) => (
          <StatCard
            key={metric.title}
            label={metric.title}
            value={fmt(metric.value)}
            unit={metric.suffix}
            description={metric.sub}
            color={metric.title === "当前电量" ? "brand" : "default"}
          />
        ))}
      </div>
      <p className="text-sm text-muted">
        读数日期：{summary?.observedDate ?? "暂无数据"}。
        {summary?.expired ? "读数已过期，无法有效预测。" : "预计可用天数仅供参考。"}{" "}
        {rate != null && `固定换算：${rate} 元/度。`}
      </p>
      {result.cache?.updatedAtMs != null && (
        <p className="text-sm text-muted">
          {result.cache.fromCache ? "已加载本地记录" : "记录已更新"} · 最近获取：
          {new Date(result.cache.updatedAtMs).toLocaleString("zh-CN", {
            timeZone: "Asia/Shanghai",
          })}
          （北京时间）。通常每天 23:59 左右更新，可手动刷新最新数据。
        </p>
      )}
      {summary?.warningReason && (
        <Banner status="warning">
          <Banner.Title>电量不足提醒</Banner.Title>
          <Banner.Description>{summary.warningReason}</Banner.Description>
        </Banner>
      )}
      <Card
        className="payments-card min-w-0"
        variant="outlined"
        headline="用电趋势"
        subhead={`${result.beginDate} 至 ${result.endDate} · 缺失数据不补零`}
      >
        <div className="p-2 sm:p-4">
          <Button variant="outlined" disabled={rate == null} onClick={() => setMoney((v) => !v)}>
            {money ? "切换为电量" : "切换为电费"}
          </Button>
          <div className="h-80 min-w-0 w-full" aria-label={`31 天用电趋势，单位${unit}`}>
            <ResponsiveContainer width="100%" height="100%" minWidth={0}>
              <ComposedChart
                data={chart}
                accessibilityLayer
                margin={{ top: 20, right: 8, left: 0, bottom: 8 }}
              >
                <CartesianGrid
                  strokeDasharray="3 3"
                  stroke="var(--mtrl-sys-color-outline-variant)"
                  opacity={0.4}
                />
                <XAxis
                  dataKey="date"
                  tickFormatter={(v: string) => v.slice(5)}
                  minTickGap={28}
                  stroke="var(--mtrl-sys-color-outline-variant)"
                  tick={{ fill: "var(--mtrl-sys-color-on-surface-variant)", fontSize: 12 }}
                />
                <YAxis
                  yAxisId="balance"
                  width={48}
                  stroke="var(--mtrl-sys-color-outline-variant)"
                  tick={{ fill: "var(--mtrl-sys-color-on-surface-variant)", fontSize: 12 }}
                />
                <YAxis
                  yAxisId="usage"
                  orientation="right"
                  width={44}
                  stroke="var(--mtrl-sys-color-outline-variant)"
                  tick={{ fill: "var(--mtrl-sys-color-on-surface-variant)", fontSize: 12 }}
                />
                <Tooltip
                  formatter={(value) => `${fmt(typeof value === "number" ? value : null)} ${unit}`}
                  contentStyle={{
                    backgroundColor: "var(--mtrl-sys-color-surface-container-high)",
                    borderColor: "var(--mtrl-sys-color-outline-variant)",
                    color: "var(--mtrl-sys-color-on-surface)",
                    borderRadius: "12px",
                    boxShadow: "0 4px 16px rgba(0,0,0,0.2)",
                  }}
                  itemStyle={{
                    color: "var(--mtrl-sys-color-on-surface)",
                  }}
                  labelStyle={{
                    color: "var(--mtrl-sys-color-on-surface-variant)",
                    fontWeight: 600,
                  }}
                />
                <Legend wrapperStyle={{ color: "var(--mtrl-sys-color-on-surface)" }} />
                <Bar
                  yAxisId="usage"
                  dataKey="dailyUsed"
                  name={money ? "每日电费" : "每日用电"}
                  fill="var(--mtrl-sys-color-primary)"
                  fillOpacity={0.7}
                  isAnimationActive={false}
                />
                <Line
                  yAxisId="balance"
                  dataKey="remaining"
                  name={money ? "折算余额" : "剩余电量"}
                  stroke="var(--mtrl-sys-color-success)"
                  strokeWidth={2}
                  connectNulls={false}
                  dot={false}
                  isAnimationActive={false}
                />
              </ComposedChart>
            </ResponsiveContainer>
          </div>
          {result.areaId === "xili-lihuo-phase2" && (
            <p className="text-sm text-muted">丽湖二期暂无历史余额，显示每日用量与当前余额。</p>
          )}
          <h3 className="electricity-section-title">用电记录</h3>
          <Table aria-label="用电记录">
            <Table.ScrollContainer className="max-h-96 overflow-auto">
              <Table.Content aria-label="用电记录" className="min-w-[360px] whitespace-nowrap">
                <Table.Header>
                  <Table.Column isRowHeader>日期</Table.Column>
                  <Table.Column>剩余（{unit}）</Table.Column>
                  <Table.Column>当日用电（{unit}）</Table.Column>
                </Table.Header>
                <Table.Body items={[...chart].reverse()}>
                  {(r) => (
                    <Table.Row id={r.date}>
                      <Table.Cell>{r.date}</Table.Cell>
                      <Table.Cell>{fmt(r.remaining)}</Table.Cell>
                      <Table.Cell>{fmt(r.dailyUsed)}</Table.Cell>
                    </Table.Row>
                  )}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
          {result.readings.length === 0 && <p>该时间段没有查询到记录。</p>}
        </div>
      </Card>
      <Card
        className="payments-card min-w-0"
        variant="outlined"
        headline="充值记录及来源"
        subhead="最近 31 天 · 金额来自充值记录"
      >
        <div className="p-2 sm:p-4">
          {result.rechargeError ? (
            <Banner status="warning">
              <Banner.Title>充值记录查询未完成</Banner.Title>
              <Banner.Description>{result.rechargeError}</Banner.Description>
            </Banner>
          ) : null}
          {result.rechargeError && (
            <Button disabled={retrying} onClick={retryRecharges}>
              {retrying ? "正在重试…" : "重试充值记录"}
            </Button>
          )}
          {result.recharges?.length ? (
            <Table aria-label="充值记录">
              <Table.ScrollContainer>
                <Table.Content aria-label="充值记录" className="min-w-[560px] whitespace-nowrap">
                  <Table.Header>
                    <Table.Column isRowHeader>时间</Table.Column>
                    <Table.Column>充值电量（度）</Table.Column>
                    <Table.Column>实际金额（元）</Table.Column>
                    <Table.Column>来源</Table.Column>
                  </Table.Header>
                  <Table.Body>
                    {result.recharges.map((r, index) => (
                      <Table.Row key={`${r.time}-${index}`} id={`${r.time}-${index}`}>
                        <Table.Cell>{r.time}</Table.Cell>
                        <Table.Cell>{fmt(r.kwh)}</Table.Cell>
                        <Table.Cell>{fmt(r.yuan)}</Table.Cell>
                        <Table.Cell>
                          {r.sourceKind === "person" ? "充值人" : "充值渠道"}：
                          {r.source || "接口未提供"}
                        </Table.Cell>
                      </Table.Row>
                    ))}
                  </Table.Body>
                </Table.Content>
              </Table.ScrollContainer>
            </Table>
          ) : (
            !result.rechargeError && <p>该时间段没有充值记录。</p>
          )}
        </div>
        <div slot="actions">
          <Button
            variant="tonal"
            onClick={() => void navigation.replace("/campus-payments/recharge")}
          >
            打开官方充值入口
          </Button>
        </div>
      </Card>
    </>
  );
}
