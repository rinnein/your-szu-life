import { Button, Card, Chip } from "@heroui/react";
import { LuPause, LuPlay, LuSquare } from "react-icons/lu";
import { useCacheMode, canSelect } from "./view-mode";
import { useEnrollment } from "./use-enrollment";
import { Choice, OperationError, phases } from "./shared";
import { gears } from "./model";
export default function TasksPage() {
  const cacheOnly = useCacheMode();
  const { snapshot: s, run, errors, pending } = useEnrollment(cacheOnly);
  const control = (
    action: string,
    id: string | null = null,
    gear: string | null = null,
    priority: number | null = null,
  ) => void run({ type: "control", action, id, gear, priority });
  return (
    <div className="grid gap-4">
      <Card>
        <Card.Content className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <p className="text-lg font-semibold">{s?.running ? "任务正在运行" : "任务已暂停"}</p>
            <p className="text-sm text-muted">
              离开页面后任务仍在后台执行。{s?.tasks.filter((t) => t.enabled).length ?? 0} 项待处理。
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              isDisabled={cacheOnly || !canSelect(s) || !!pending.control}
              onPress={() => control("start")}
            >
              <LuPlay />
              开始 / 继续
            </Button>
            <Button variant="secondary" onPress={() => control("pause")}>
              <LuPause />
              暂停
            </Button>
            <Button variant="tertiary" onPress={() => control("stop")}>
              <LuSquare />
              停止全部
            </Button>
          </div>
        </Card.Content>
      </Card>
      <OperationError message={errors.control} />
      {s?.message && (
        <p className="text-sm text-muted" role="status">
          {s.message}
        </p>
      )}
      {s?.tasks.map((t) => (
        <Card key={t.id}>
          <Card.Header>
            <div className="flex items-start justify-between gap-3">
              <div>
                <Card.Title>{t.course.name}</Card.Title>
                <Card.Description>
                  {t.oldCourse
                    ? `从「${t.oldCourse.name}」换入 · 余量达到 ${t.threshold} 时触发`
                    : t.group
                      ? `备选组：${t.group}`
                      : "独立目标"}
                </Card.Description>
              </div>
              <Chip size="sm" variant="soft">
                {phases[t.phase] ?? t.phase}
              </Chip>
            </div>
          </Card.Header>
          <Card.Content className="grid gap-3 sm:grid-cols-[180px_1fr]">
            <Choice
              label="执行挡位"
              disabled={cacheOnly}
              value={t.gear}
              onChange={(g) => control("configure", t.id, g)}
              items={Object.entries(gears).map(([id, label]) => ({ id, label }))}
            />
            <div className="text-sm text-muted">
              <p>
                优先级 {t.priority} · 当前挡位已失败 {t.failures} 次
              </p>
              {t.startedAt && <p>预约时间 {new Date(t.startedAt).toLocaleString()}</p>}
              <p>{t.message}</p>
            </div>
          </Card.Content>
          <Card.Footer className="flex-wrap gap-2">
            <Button
              size="sm"
              variant="secondary"
              onPress={() => control("configure", t.id, null, t.priority + 1)}
            >
              提高优先级
            </Button>
            <Button size="sm" variant="secondary" onPress={() => control("stop", t.id)}>
              停止此项
            </Button>
            <Button
              size="sm"
              variant="tertiary"
              isDisabled={cacheOnly || !!t.pending}
              onPress={() => control("remove", t.id)}
            >
              移除
            </Button>
          </Card.Footer>
        </Card>
      ))}
      {!s?.tasks.length && (
        <Card>
          <Card.Content className="py-12 text-center text-muted">
            还没有待抢任务。在课程目录中选择课程，加入待抢列表。
          </Card.Content>
        </Card>
      )}
      <p className="text-xs text-muted">
        ultra 明确失败 5 次降至 high，high 明确失败 30 次降至 medium；网络超时不计数。
      </p>
    </div>
  );
}
