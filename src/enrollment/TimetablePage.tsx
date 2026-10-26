import { useState } from "react";
import { Card } from "material/react";
import { Chip, Table } from "../ui";
import { useCacheMode } from "./view-mode";
import { useEnrollment } from "./use-enrollment";
import { Choice } from "./shared";
import { conflicts, timetable } from "./model";
export default function TimetablePage() {
  const cacheOnly = useCacheMode();
  const { snapshot: s } = useEnrollment(cacheOnly);
  const [week, setWeek] = useState("all");
  const data = s ? timetable(s.selected, s.tasks) : { slots: [], unscheduled: [] };
  const slots = data.slots.filter((slot) => week === "all" || slot.weeks.includes(Number(week)));
  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex gap-2">
          <Chip variant="soft" color="accent">
            已选课程
          </Chip>
          <Chip variant="soft">待抢课程</Chip>
          <p className="self-center text-sm text-muted">重叠课程保留显示</p>
        </div>
        <div className="w-44">
          <Choice
            label="显示周次"
            value={week}
            onChange={setWeek}
            items={[
              { id: "all", label: "全部周次" },
              ...Array.from({ length: 30 }, (_, i) => ({
                id: String(i + 1),
                label: `第 ${i + 1} 周`,
              })),
            ]}
          />
        </div>
      </div>
      <Card variant="outlined">
        <div className="p-0 overflow-hidden">
          <Table>
            <Table.ScrollContainer>
              <Table.Content aria-label="选课期间课表" className="min-w-[1000px]">
                <Table.Header>
                  <Table.Column isRowHeader>节次</Table.Column>
                  {Array.from("一二三四五六日", (day) => (
                    <Table.Column key={day}>星期{day}</Table.Column>
                  ))}
                </Table.Header>
                <Table.Body>
                  {Array.from({ length: 14 }, (_, p) => (
                    <Table.Row key={p}>
                      <Table.Cell>第{p + 1}节</Table.Cell>
                      {Array.from({ length: 7 }, (_, d) => (
                        <Table.Cell key={d} className="min-w-32 align-top">
                          {slots
                            .filter((a) => a.day === d + 1 && a.start <= p + 1 && a.end >= p + 1)
                            .map((a) => (
                              <div
                                key={`${a.course.id}-${a.start}`}
                                className={`my-1 rounded-lg p-2 text-xs ${a.pending ? "bg-default" : "bg-accent-soft"}`}
                              >
                                <p className="font-semibold">{a.course.name}</p>
                                <p>
                                  {a.pending ? "待抢" : "已选"} · {a.start}–{a.end}节
                                </p>
                                <p className="text-muted">{a.course.teacher}</p>
                                {slots.some((b) => conflicts(a, b)) && (
                                  <p className="text-danger">时间冲突</p>
                                )}
                              </div>
                            ))}
                        </Table.Cell>
                      ))}
                    </Table.Row>
                  ))}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
        </div>
      </Card>
      {data.unscheduled.length > 0 && (
        <Card variant="outlined" headline="尚未排入课表">
          <div className="p-4 pt-0">
            {data.unscheduled.map((c) => (
              <p key={c.id}>
                {c.name} · {c.teacher} · {c.schedule}
              </p>
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}
