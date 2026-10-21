import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Alert,
  AlertDialog,
  Button,
  Card,
  Chip,
  Label,
  ListBox,
  Select,
  Spinner,
  toast,
} from "@heroui/react";
import { LuBookOpen, LuCalendar, LuCircleCheck, LuClock, LuRotateCw } from "react-icons/lu";
import { ExamPaperModal } from "./ExamPaperModal";
import type { ExamTask, UoocCourse } from "./types";

interface ExamListPanelProps {
  courses: UoocCourse[];
  ready: boolean;
}

export function ExamListPanel({ courses, ready }: ExamListPanelProps) {
  const [exams, setExams] = useState<ExamTask[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedCid, setSelectedCid] = useState<string>("all");
  const [activeExam, setActiveExam] = useState<ExamTask | null>(null);
  const [examToStart, setExamToStart] = useState<ExamTask | null>(null);

  const fetchExams = async (cidOption?: string) => {
    if (!ready) return;
    setLoading(true);
    const targetCid = cidOption === "all" ? undefined : cidOption || undefined;
    try {
      const res = await invoke<ExamTask[]>("list_uooc_exams", {
        cid: targetCid || null,
      });
      setExams(res || []);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "拉取考试列表失败";
      toast.danger(msg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (ready) {
      void fetchExams(selectedCid);
    }
  }, [ready, selectedCid]);

  const getExamDisplayStatus = (exam: ExamTask) => {
    const now = new Date().getTime();
    const start = exam.startTime ? new Date(exam.startTime).getTime() : 0;
    const end = exam.endTime ? new Date(exam.endTime).getTime() : 0;

    const isGradedOrSubmitted =
      exam.canView ||
      (exam.statusCode !== undefined && exam.statusCode >= 20) ||
      exam.statusText === "已批改" ||
      (exam.score !== null && exam.score !== undefined) ||
      (exam.chance > 0 && exam.submitCount >= exam.chance);

    if (isGradedOrSubmitted) {
      return {
        label:
          exam.score !== null && exam.score !== undefined
            ? `已批改: ${exam.score}分`
            : exam.statusText || "已交卷",
        color: "success" as const,
        actionLabel: "查看答卷",
        canEnter: true,
        isSubmitted: true,
      };
    }

    if (end > 0 && now > end && !exam.canDo) {
      return {
        label: "已逾期",
        color: "default" as const,
        actionLabel: exam.canView ? "查看答卷" : "已截止",
        canEnter: !!exam.canView,
        isSubmitted: false,
      };
    }

    if (start > 0 && now < start && !exam.canDo) {
      return {
        label: "未开始",
        color: "warning" as const,
        actionLabel: "未到期",
        canEnter: false,
        isSubmitted: false,
      };
    }

    return {
      label: exam.statusText || "进行中",
      color: "accent" as const,
      actionLabel: "进入考试",
      canEnter: true,
      isSubmitted: false,
    };
  };

  const handleExamActionClick = (exam: ExamTask) => {
    const status = getExamDisplayStatus(exam);
    if (!status.canEnter) return;

    if (status.isSubmitted) {
      // 已经提交查看答卷无需倒计时警告，直接打开
      setActiveExam(exam);
    } else {
      // 未提交进入考试：弹出强二次确认，防止误触发倒计时
      setExamToStart(exam);
    }
  };

  return (
    <div className="grid gap-6 pt-4">
      {/* 顶部控制栏 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2.5 w-full sm:w-auto">
          <Select
            aria-label="选择课程筛选"
            className="w-full sm:w-64"
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
            onPress={() => void fetchExams(selectedCid)}
          >
            {loading ? <Spinner size="sm" /> : <LuRotateCw />}
            刷新列表
          </Button>
        </div>

        <p className="text-sm text-muted">
          共收录 {exams.length} 场考试任务 · 考试交卷不可逆，请在答题面板谨慎确认
        </p>
      </div>

      {/* 提示信息 */}
      <Alert status="accent" className="flex items-center gap-2 text-xs">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>考试说明</Alert.Title>
          <Alert.Description>
            考试任务独立于章节课后测验，支持限时倒计时、草稿自动暂存、防作弊心跳保持与题库向量匹配。进入考试后不会触发官方切屏上报。
          </Alert.Description>
        </Alert.Content>
      </Alert>

      {/* 列表内容 */}
      {loading && exams.length === 0 ? (
        <div className="flex h-48 flex-col items-center justify-center gap-3">
          <Spinner size="lg" />
          <p className="text-sm text-muted">正在检索课程考试任务...</p>
        </div>
      ) : exams.length === 0 ? (
        <Card className="p-8 text-center">
          <p className="text-muted">当前课程下暂无开放或收录的考试任务</p>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {exams.map((exam) => {
            const statusInfo = getExamDisplayStatus(exam);

            return (
              <Card key={exam.id} className="flex flex-col justify-between border border-border/60">
                <Card.Header className="flex flex-wrap items-start justify-between gap-2 pb-2">
                  <div className="space-y-1">
                    <Card.Title className="text-base font-bold leading-snug">
                      {exam.title}
                    </Card.Title>
                    <p className="flex items-center gap-1 text-xs text-muted">
                      <LuBookOpen className="h-3 w-3" />
                      {exam.courseName}
                    </p>
                  </div>

                  <Chip variant="soft" color={statusInfo.color} size="sm">
                    {statusInfo.label}
                  </Chip>
                </Card.Header>

                <Card.Content className="space-y-2 py-2 text-xs text-muted">
                  <div className="flex items-center gap-2">
                    <LuCalendar className="h-3.5 w-3.5" />
                    <span>起止时间: {exam.startTime || "未定"} ~ {exam.endTime || "未定"}</span>
                  </div>

                  <div className="flex flex-wrap items-center gap-4">
                    <span className="flex items-center gap-1">
                      <LuClock className="h-3.5 w-3.5" />
                      时长: {exam.limitTime > 0 ? `${exam.limitTime} 分钟` : "不限时"}
                    </span>

                    <span className="flex items-center gap-1">
                      <LuCircleCheck className="h-3.5 w-3.5" />
                      作答机会: {exam.chance > 0 ? `已考 ${exam.submitCount} / 共 ${exam.chance} 次` : "不限次数"}
                    </span>
                  </div>
                </Card.Content>

                <Card.Footer className="flex items-center justify-between border-t bg-surface/30 pt-3">
                  <span className="font-mono text-xs text-muted">ID: {exam.taskId}</span>

                  <Button
                    variant="primary"
                    size="sm"
                    isDisabled={!statusInfo.canEnter}
                    onPress={() => handleExamActionClick(exam)}
                  >
                    {statusInfo.actionLabel}
                  </Button>
                </Card.Footer>
              </Card>
            );
          })}
        </div>
      )}

      {/* 考试开始前二次确认弹窗 (HeroUI AlertDialog) */}
      <AlertDialog isOpen={!!examToStart} onOpenChange={(open) => !open && setExamToStart(null)}>
        <AlertDialog.Backdrop>
          <AlertDialog.Container size="sm">
            <AlertDialog.Dialog className="max-w-md">
              <AlertDialog.Icon status="warning" />
              <AlertDialog.Header>
                <AlertDialog.Heading>确认现在开始考试？</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body className="space-y-3">
                <p className="text-sm">
                  您即将开始：<strong>{examToStart?.title}</strong>
                </p>
                <Alert status="warning">
                  <Alert.Indicator />
                  <Alert.Content>
                    <Alert.Title>注意倒计时触发！</Alert.Title>
                    <Alert.Description>
                      {examToStart?.limitTime && examToStart.limitTime > 0
                        ? `该考试设置有限时 ${examToStart.limitTime} 分钟。一旦确认开始，试卷倒计时将立即触发且不可暂停！`
                        : "该考试虽未设置严格单次限时，但进入后将开始记录考试作答，请确保当前时间充裕。"}
                    </Alert.Description>
                  </Alert.Content>
                </Alert>
                <p className="text-xs text-muted">
                  请确认您处于安静且网络稳定的环境。如尚未准备好，请点击“稍后再考”。
                </p>
              </AlertDialog.Body>
              <AlertDialog.Footer className="flex justify-end gap-2">
                <AlertDialog.CloseTrigger className="dialog-cancel" aria-label="稍后再考" />
                <Button
                  variant="primary"
                  onPress={() => {
                    const exam = examToStart;
                    setExamToStart(null);
                    if (exam) {
                      setActiveExam(exam);
                    }
                  }}
                >
                  确认开始
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      </AlertDialog>

      {/* 考试答题模态框 */}
      {activeExam && (
        <ExamPaperModal
          exam={activeExam}
          isOpen={!!activeExam}
          onClose={() => setActiveExam(null)}
          onSubmitted={() => void fetchExams(selectedCid)}
        />
      )}
    </div>
  );
}
