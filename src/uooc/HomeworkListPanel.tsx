import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Alert,
  Button,
  Card,
  Chip,
  Label,
  ListBox,
  Select,
  Spinner,
  toast,
} from "@heroui/react";
import {
  LuBookOpen,
  LuCalendar,
  LuCircleCheck,
  LuRotateCw,
  LuUsers,
} from "react-icons/lu";
import { ExamPaperModal } from "./ExamPaperModal";
import { PeerEvalModal } from "./PeerEvalModal";
import type { HomeworkTask, UoocCourse } from "./types";

interface HomeworkListPanelProps {
  courses: UoocCourse[];
  ready: boolean;
}

export function HomeworkListPanel({ courses, ready }: HomeworkListPanelProps) {
  const [homeworks, setHomeworks] = useState<HomeworkTask[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedCid, setSelectedCid] = useState<string>("all");
  const [activeHomework, setActiveHomework] = useState<HomeworkTask | null>(null);
  const [evalModalOpen, setEvalModalOpen] = useState(false);
  const [evalCid, setEvalCid] = useState<string | undefined>(undefined);

  const fetchHomeworks = async (cidOption?: string) => {
    if (!ready) return;
    setLoading(true);
    const targetCid = cidOption === "all" ? undefined : cidOption || undefined;
    try {
      const res = await invoke<HomeworkTask[]>("list_uooc_homeworks", {
        cid: targetCid || null,
      });
      setHomeworks(res || []);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "拉取作业列表失败";
      toast.danger(msg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (ready) {
      void fetchHomeworks(selectedCid);
    }
  }, [ready, selectedCid]);

  const getHomeworkDisplayStatus = (hw: HomeworkTask) => {
    const now = new Date().getTime();
    const start = hw.startTime ? new Date(hw.startTime).getTime() : 0;
    const end = hw.endTime ? new Date(hw.endTime).getTime() : 0;

    const isGradedOrSubmitted =
      hw.canView ||
      (hw.statusCode !== undefined && hw.statusCode >= 20) ||
      hw.statusText === "已批改" ||
      (hw.score !== null && hw.score !== undefined) ||
      (hw.chance > 0 && hw.submitCount >= hw.chance);

    if (isGradedOrSubmitted) {
      return {
        label:
          hw.score !== null && hw.score !== undefined
            ? `已批改: ${hw.score}分`
            : hw.statusText || "已提交",
        color: "success" as const,
        actionLabel: "查看答卷",
        canEnter: true,
      };
    }

    if (end > 0 && now > end && !hw.canDo) {
      return {
        label: "已截止",
        color: "default" as const,
        actionLabel: hw.canView ? "查看详情" : "已截止",
        canEnter: !!hw.canView,
      };
    }

    if (start > 0 && now < start && !hw.canDo) {
      return {
        label: "未开始",
        color: "warning" as const,
        actionLabel: "未到期",
        canEnter: false,
      };
    }

    if (hw.statusText === "待批改" || hw.statusCode === 10) {
      return {
        label: "待批改",
        color: "warning" as const,
        actionLabel: "查看详情",
        canEnter: true,
      };
    }

    return {
      label: hw.statusText || "作答中",
      color: "accent" as const,
      actionLabel: "进入作业",
      canEnter: true,
    };
  };

  return (
    <div className="grid gap-6 pt-4">
      {/* 顶部控制栏 */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <Select
            aria-label="选择课程筛选"
            className="w-64"
            value={selectedCid}
            onChange={(val) => {
              if (typeof val === "string") setSelectedCid(val);
            }}
          >
            <Label className="sr-only">课程筛选</Label>
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                <ListBox.Item id="all" textValue="全部开课中的课程">
                  全部开课中的课程
                  <ListBox.ItemIndicator />
                </ListBox.Item>
                {courses.map((c) => (
                  <ListBox.Item key={c.id} id={c.id} textValue={c.title}>
                    {c.title}
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>

          <Button
            variant="secondary"
            isDisabled={loading || !ready}
            onPress={() => void fetchHomeworks(selectedCid)}
          >
            {loading ? <Spinner size="sm" /> : <LuRotateCw />}
            刷新列表
          </Button>

          <Button
            variant="secondary"
            isDisabled={!ready}
            onPress={() => {
              setEvalCid(selectedCid === "all" ? undefined : selectedCid);
              setEvalModalOpen(true);
            }}
          >
            <LuUsers className="text-accent" />
            互评助手
          </Button>
        </div>

        <p className="text-sm text-muted">
          共收录 {homeworks.length} 项作业任务 · 支持客观题题库单题匹配、主观题 AI 答题与互评
        </p>
      </div>

      {/* 提示信息 */}
      <Alert status="accent" className="flex items-center gap-2 text-xs">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>作业与互评说明</Alert.Title>
          <Alert.Description>
            课程作业支持单题题库反查与向量特征匹配。主观问答题支持一键【AI 解答】生成结构化草稿；涉及附件上传时可通过官网窗口处理；若教师启用了互评，请在互评期内通过【互评助手】评阅。
          </Alert.Description>
        </Alert.Content>
      </Alert>

      {/* 列表内容 */}
      {loading && homeworks.length === 0 ? (
        <div className="flex h-48 flex-col items-center justify-center gap-3">
          <Spinner size="lg" />
          <p className="text-sm text-muted">正在检索课程作业任务...</p>
        </div>
      ) : homeworks.length === 0 ? (
        <Card className="p-8 text-center">
          <p className="text-muted">当前课程下暂无开放或收录的作业任务</p>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {homeworks.map((hw) => {
            const statusInfo = getHomeworkDisplayStatus(hw);

            return (
              <Card key={hw.id} className="flex flex-col justify-between border border-border/60">
                <Card.Header className="flex flex-wrap items-start justify-between gap-2 pb-2">
                  <div className="space-y-1">
                    <Card.Title className="text-base font-bold leading-snug">
                      {hw.title}
                    </Card.Title>
                    <p className="flex items-center gap-1 text-xs text-muted">
                      <LuBookOpen className="h-3 w-3" />
                      {hw.courseName}
                    </p>
                  </div>

                  <Chip variant="soft" color={statusInfo.color} size="sm">
                    {statusInfo.label}
                  </Chip>
                </Card.Header>

                <Card.Content className="space-y-2 py-2 text-xs text-muted">
                  <div className="flex items-center gap-2">
                    <LuCalendar className="h-3.5 w-3.5" />
                    <span>起止时间: {hw.startTime || "未定"} ~ {hw.endTime || "未定"}</span>
                  </div>

                  <div className="flex flex-wrap items-center gap-4">
                    <span className="flex items-center gap-1">
                      <LuCircleCheck className="h-3.5 w-3.5" />
                      提交次数: {hw.chance > 0 ? `已交 ${hw.submitCount} / 共 ${hw.chance} 次` : "不限次数"}
                    </span>

                    {hw.hasEval && (
                      <span className="flex items-center gap-1 text-accent">
                        <LuUsers className="h-3.5 w-3.5" />
                        含互评环节
                      </span>
                    )}
                  </div>
                </Card.Content>

                <Card.Footer className="flex items-center justify-between border-t bg-surface/30 pt-3">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-xs text-muted">ID: {hw.taskId}</span>
                    {hw.hasEval && (
                      <Button
                        variant="tertiary"
                        size="sm"
                        onPress={() => {
                          setEvalCid(hw.cid);
                          setEvalModalOpen(true);
                        }}
                        className="h-6 px-2 text-[11px]"
                      >
                        <LuUsers className="text-accent" />
                        去互评
                      </Button>
                    )}
                  </div>

                  <Button
                    variant="primary"
                    size="sm"
                    isDisabled={!statusInfo.canEnter}
                    onPress={() => setActiveHomework(hw)}
                  >
                    {statusInfo.actionLabel}
                  </Button>
                </Card.Footer>
              </Card>
            );
          })}
        </div>
      )}

      {/* 作业答题模态框 */}
      {activeHomework && (
        <ExamPaperModal
          exam={activeHomework}
          isOpen={!!activeHomework}
          onClose={() => setActiveHomework(null)}
          onSubmitted={() => void fetchHomeworks(selectedCid)}
          isHomework={true}
        />
      )}

      {/* 互评助手模态框 */}
      <PeerEvalModal
        isOpen={evalModalOpen}
        onClose={() => setEvalModalOpen(false)}
        courses={courses}
        defaultCid={evalCid}
      />
    </div>
  );
}
