import { useEffect, useState } from "react";
import { Card, Button } from "material/react";
import { Table } from "../ui";
import { useCacheMode, canSelect } from "./view-mode";
import { useEnrollment } from "./use-enrollment";
import { OperationError } from "./shared";
import CourseActionDialog from "./CourseActionDialog";
import type { Course } from "./model";
import { useLogicalNavigation } from "../logical-navigation";
export default function SelectedPage() {
  const cacheOnly = useCacheMode();
  const { snapshot: s, run, pending, errors } = useEnrollment(cacheOnly);
  const nav = useLogicalNavigation();
  const [selection, setSelection] = useState<{ course: Course; drop: boolean }>();
  useEffect(() => {
    if (cacheOnly || canSelect(s)) void run({ type: "selected" });
  }, [s?.loggedIn, s?.admission, cacheOnly, run]);
  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-muted">已选 {s?.selected.length ?? 0} 个教学班</p>
        <div className="flex gap-2">
          <Button variant="tonal" onClick={() => void nav.back()}>
            查找新课 / 配置换课
          </Button>
          <Button disabled={!!pending.selected} onClick={() => void run({ type: "selected" })}>
            {cacheOnly ? "读取已选缓存" : "刷新已选"}
          </Button>
        </div>
      </div>
      <OperationError message={errors.selected || errors.drop} />
      <Card variant="outlined">
        <div className="p-4">
          <Table>
            <Table.ScrollContainer>
              <Table.Content aria-label="已选课程" className="min-w-[720px]">
                <Table.Header>
                  <Table.Column isRowHeader>课程</Table.Column>
                  <Table.Column>教师</Table.Column>
                  <Table.Column>学分</Table.Column>
                  <Table.Column>时间地点</Table.Column>
                  <Table.Column>操作</Table.Column>
                </Table.Header>
                <Table.Body renderEmptyState={() => "暂无已选课程"}>
                  {s?.selected.map((c) => (
                    <Table.Row key={c.id}>
                      <Table.Cell>
                        <p>{c.name}</p>
                        <p className="text-xs text-muted">{c.code}</p>
                      </Table.Cell>
                      <Table.Cell>{c.teacher}</Table.Cell>
                      <Table.Cell>{c.credits ?? "未知"}</Table.Cell>
                      <Table.Cell className="max-w-[300px] whitespace-normal">
                        {c.schedule}
                      </Table.Cell>
                      <Table.Cell>
                        <Button
                          className="button-danger-soft"
                          size="xs"
                          variant="tonal"
                          disabled={cacheOnly || !canSelect(s)}
                          onClick={() => setSelection({ course: c, drop: true })}
                        >
                          退课
                        </Button>
                      </Table.Cell>
                    </Table.Row>
                  ))}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
        </div>
      </Card>
      {!cacheOnly && (
        <CourseActionDialog selection={selection} onClose={() => setSelection(undefined)} />
      )}
    </div>
  );
}
