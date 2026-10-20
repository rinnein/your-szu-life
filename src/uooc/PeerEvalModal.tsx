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
  Modal,
  NumberField,
  Select,
  Spinner,
  TextArea,
  toast,
} from "@heroui/react";
import { LuCheck, LuSend, LuSparkles, LuUsers } from "react-icons/lu";
import type { PeerEvalTask, PeerSubmission, UoocCourse } from "./types";

interface PeerEvalModalProps {
  isOpen: boolean;
  onClose: () => void;
  courses: UoocCourse[];
  defaultCid?: string;
}

export function PeerEvalModal({
  isOpen,
  onClose,
  courses,
  defaultCid,
}: PeerEvalModalProps) {
  const [selectedCid, setSelectedCid] = useState<string>(() => defaultCid || courses[0]?.id || "");
  const [loading, setLoading] = useState(false);
  const [tasks, setTasks] = useState<PeerEvalTask[]>([]);
  const [scores, setScores] = useState<Record<string, number>>({});
  const [comments, setComments] = useState<Record<string, string>>({});
  const [submittingId, setSubmittingId] = useState<string | null>(null);
  const [generatingAiId, setGeneratingAiId] = useState<string | null>(null);
  const [batchDialogOpen, setBatchDialogOpen] = useState(false);
  const [batchBusy, setBatchBusy] = useState(false);

  useEffect(() => {
    if (defaultCid) {
      setSelectedCid(defaultCid);
    } else if (courses.length > 0 && !selectedCid) {
      setSelectedCid(courses[0].id);
    }
  }, [defaultCid, courses, selectedCid]);

  const fetchEvals = async (cid: string) => {
    if (!cid) return;
    setLoading(true);
    try {
      const res = await invoke<PeerEvalTask[]>("get_uooc_eval_list", {
        cid,
      });
      setTasks(res || []);
      const initScores: Record<string, number> = {};
      const initComments: Record<string, string> = {};
      for (const t of res || []) {
        for (const s of t.submissions) {
          initScores[s.evalId] = s.score ?? 95;
          initComments[s.evalId] = s.comments || "作业完成认真规范，思路清晰，重点突出，整体质量优秀。";
        }
      }
      setScores(initScores);
      setComments(initComments);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "获取互评任务失败";
      toast.danger(msg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen && selectedCid) {
      void fetchEvals(selectedCid);
    }
  }, [isOpen, selectedCid]);

  const handleGenerateComment = async (sub: PeerSubmission) => {
    setGeneratingAiId(sub.evalId);
    try {
      const prompt = `请针对以下同学提交的课程作业内容，撰写一段约50字、真诚、客观、积极正面的简短互评评语：\n${sub.content || "（该作业为标准文档/主观解答）"}`;
      const generated = await invoke<string>("generate_uooc_subjective", {
        stem: prompt,
        context: null,
        referenceExamples: null,
      });
      if (generated && generated.trim().length > 0) {
        setComments((prev) => ({
          ...prev,
          [sub.evalId]: generated.trim(),
        }));
        toast.success("AI 评语生成成功");
      }
    } catch {
      // AI 失败时使用高质量缺省正面评语
      setComments((prev) => ({
        ...prev,
        [sub.evalId]: "论述条理清晰，结构完整，紧扣主题，书写认真，完成质量很高。",
      }));
      toast.info("已填入推荐正面评语");
    } finally {
      setGeneratingAiId(null);
    }
  };

  const handleSubmitSingle = async (tid: string, sub: PeerSubmission) => {
    const score = scores[sub.evalId] ?? 95;
    const comment = comments[sub.evalId] ?? "作业完成质量优秀。";
    setSubmittingId(sub.evalId);
    try {
      const msg = await invoke<string>("submit_uooc_eval", {
        cid: selectedCid,
        tid,
        evalId: sub.evalId,
        score,
        comments: comment,
      });
      toast.success(msg || "互评已成功提交");
      void fetchEvals(selectedCid);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "提交互评失败";
      toast.danger(msg);
    } finally {
      setSubmittingId(null);
    }
  };

  const handleBatchSubmit = async () => {
    setBatchBusy(true);
    let successCount = 0;
    try {
      for (const t of tasks) {
        for (const s of t.submissions) {
          if (s.isEvaluated) continue;
          const score = scores[s.evalId] ?? 95;
          const comment = comments[s.evalId] ?? "作业完成规范，论述清晰，质量优秀。";
          await invoke("submit_uooc_eval", {
            cid: selectedCid,
            tid: t.tid,
            evalId: s.evalId,
            score,
            comments: comment,
          });
          successCount += 1;
        }
      }
      toast.success(`已批量完成 ${successCount} 篇互评任务`);
      setBatchDialogOpen(false);
      void fetchEvals(selectedCid);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "批量提交过程中出现错误";
      toast.danger(msg);
    } finally {
      setBatchBusy(false);
    }
  };

  const allSubmissions = tasks.flatMap((t) => t.submissions);
  const pendingCount = allSubmissions.filter((s) => !s.isEvaluated).length;

  return (
    <>
      <Modal isOpen={isOpen} onOpenChange={(open) => !open && onClose()}>
        <Modal.Backdrop>
          <Modal.Container size="cover" className="max-w-3xl">
            <Modal.Dialog aria-label="课程作业互评助手" className="flex h-[85vh] max-h-[85vh] flex-col overflow-hidden">
              <Modal.CloseTrigger aria-label="关闭互评助手" />
              <Modal.Header className="flex flex-wrap items-center justify-between border-b pb-3 pr-10">
                <div className="flex items-center gap-2">
                  <LuUsers className="h-5 w-5 text-accent" />
                  <Modal.Heading className="text-lg font-bold">课程作业互评助手</Modal.Heading>
                </div>

                <div className="flex items-center gap-2">
                  <Select
                    aria-label="选择课程"
                    selectedKey={selectedCid}
                    onSelectionChange={(key) => setSelectedCid(String(key))}
                    className="w-48 text-xs"
                  >
                    <Select.Trigger />
                    <Select.Popover>
                      <ListBox>
                        {courses.map((c) => (
                          <ListBox.Item key={c.id} id={c.id} textValue={c.title}>
                            {c.title}
                          </ListBox.Item>
                        ))}
                      </ListBox>
                    </Select.Popover>
                  </Select>
                </div>
              </Modal.Header>

              <Modal.Body className="flex-1 overflow-y-auto px-6 py-4">
                {loading && (
                  <div className="flex h-48 flex-col items-center justify-center gap-2">
                    <Spinner size="md" />
                    <p className="text-xs text-muted">正在检索当前课程的互评任务...</p>
                  </div>
                )}

                {!loading && tasks.length === 0 && (
                  <Alert status="default">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>暂无待互评任务</Alert.Title>
                      <Alert.Description>
                        当前课程未开启互评机制，或尚未到达教师设定的作业互评窗口期。
                      </Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}

                {!loading && tasks.length > 0 && (
                  <div className="space-y-6">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="text-xs text-muted">
                        共 {allSubmissions.length} 份被评作业，待评 {pendingCount} 份
                      </span>

                      {pendingCount > 0 && (
                        <Button
                          size="sm"
                          variant="secondary"
                          onPress={() => setBatchDialogOpen(true)}
                          className="text-xs"
                        >
                          <LuSparkles className="text-accent" />
                          一键完成全部待评 ({pendingCount})
                        </Button>
                      )}
                    </div>

                    {tasks.map((task) => (
                      <div key={task.tid} className="space-y-3">
                        <div className="flex items-center justify-between">
                          <h3 className="text-sm font-semibold text-foreground">
                            {task.title}
                          </h3>
                          <Chip size="sm" variant="soft">
                            已评 {task.completedCount} / {task.totalCount}
                          </Chip>
                        </div>

                        {task.submissions.map((sub, idx) => (
                          <Card key={sub.evalId} className="border border-border/60">
                            <Card.Header className="flex flex-wrap items-center justify-between gap-2 pb-2">
                              <span className="text-xs font-semibold text-accent">
                                同学作答样本 #{idx + 1}
                              </span>

                              {sub.isEvaluated ? (
                                <Chip size="sm" color="success">
                                  <LuCheck />
                                  已评分: {sub.score}分
                                </Chip>
                              ) : (
                                <Chip size="sm" variant="secondary">
                                  待评分
                                </Chip>
                              )}
                            </Card.Header>

                            <Card.Content className="space-y-3 pt-1">
                              <div className="rounded-lg bg-surface-secondary/40 p-3 text-xs leading-relaxed text-foreground/90">
                                {sub.content ? (
                                  <p className="whitespace-pre-wrap">{sub.content}</p>
                                ) : (
                                  <p className="italic text-muted">
                                    （该作业为选择题或附件形式提交，可直接客观评分并撰写评语）
                                  </p>
                                )}
                              </div>

                              <div className="grid gap-3 sm:grid-cols-2">
                                <NumberField
                                  minValue={0}
                                  maxValue={100}
                                  value={scores[sub.evalId] ?? 95}
                                  onChange={(val) =>
                                    setScores((prev) => ({ ...prev, [sub.evalId]: val }))
                                  }
                                  isDisabled={sub.isEvaluated}
                                >
                                  <Label className="text-xs font-medium">评分 (满分100)</Label>
                                  <NumberField.Group>
                                    <NumberField.DecrementButton />
                                    <NumberField.Input />
                                    <NumberField.IncrementButton />
                                  </NumberField.Group>
                                </NumberField>

                                <div className="space-y-1">
                                  <div className="flex items-center justify-between">
                                    <Label className="text-xs font-medium">互评评语</Label>
                                    {!sub.isEvaluated && (
                                      <Button
                                        size="sm"
                                        variant="tertiary"
                                        isDisabled={generatingAiId === sub.evalId}
                                        onPress={() => void handleGenerateComment(sub)}
                                        className="h-6 px-2 text-[11px]"
                                      >
                                        <LuSparkles className="text-accent" />
                                        AI 生成评语
                                      </Button>
                                    )}
                                  </div>
                                  <TextArea
                                    value={comments[sub.evalId] ?? ""}
                                    onChange={(e) =>
                                      setComments((prev) => ({
                                        ...prev,
                                        [sub.evalId]: e.target.value,
                                      }))
                                    }
                                    disabled={sub.isEvaluated}
                                    rows={2}
                                    placeholder="输入正面鼓励或建议评语…"
                                    className="text-xs font-normal"
                                  />
                                </div>
                              </div>

                              {!sub.isEvaluated && (
                                <div className="flex justify-end pt-1">
                                  <Button
                                    size="sm"
                                    isDisabled={submittingId === sub.evalId}
                                    onPress={() => void handleSubmitSingle(task.tid, sub)}
                                  >
                                    <LuSend />
                                    提交此篇评分
                                  </Button>
                                </div>
                              )}
                            </Card.Content>
                          </Card>
                        ))}
                      </div>
                    ))}
                  </div>
                )}
              </Modal.Body>

              <Modal.Footer className="flex justify-end border-t bg-surface/50 p-4">
                <Button variant="tertiary" onPress={onClose}>
                  关闭
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>

      <AlertDialog isOpen={batchDialogOpen} onOpenChange={setBatchDialogOpen}>
        <AlertDialog.Backdrop>
          <AlertDialog.Container size="sm">
            <AlertDialog.Dialog>
              <AlertDialog.Icon status="accent" />
              <AlertDialog.Header>
                <AlertDialog.Heading>确认批量完成互评？</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body>
                将为当前课程所有待评的 {pendingCount} 篇同学作业填入推荐高分（95分）与积极肯定的正面评语并正式提交。互评成绩将立即计入您的平时成绩。
              </AlertDialog.Body>
              <AlertDialog.Footer>
                <AlertDialog.CloseTrigger />
                <Button
                  isDisabled={batchBusy}
                  onPress={() => void handleBatchSubmit()}
                >
                  {batchBusy ? <Spinner size="sm" /> : <LuCheck />}
                  确认提交
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      </AlertDialog>
    </>
  );
}
