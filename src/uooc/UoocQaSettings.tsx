import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { DEFAULT_QA_SERVER_URL } from "./constants";
import {
  Alert,
  AlertDialog,
  Button,
  ButtonGroup,
  Card,
  Chip,
  Description,
  Input,
  Label,
  ListBox,
  Modal,
  Pagination,
  Select,
  Spinner,
  Switch,
  TextField,
  toast,
} from "@heroui/react";
import {
  LuCircleCheck,
  LuClock,
  LuDownload,
  LuExternalLink,
  LuFileText,
  LuFilter,
  LuKey,
  LuLaptop,
  LuLayers,
  LuRefreshCw,
  LuSearch,
  LuShieldAlert,
  LuShieldCheck,
  LuTrash2,
  LuUpload,
  LuUser,
} from "react-icons/lu";
import { openPublicUrl } from "../app-content/external";
import { getQaAuthClient, normalizeQaBaseUrl } from "./qaAuthClient";
import type { LocalBankQuestion, QuizState, UoocCourse } from "./types";
import { BankRebuildModal } from "./BankRebuildModal";
import { DimmedHtmlText, useHtmlTagVisibility } from "./DimmedHtmlText";
import { stripHtml } from "./QuizPanel";

export type EmbeddingVectorStatus = {
  status: "ready" | "empty" | "downloading" | string;
  cachedCount: number;
  totalDimension: number;
  updatedAtMs: number;
  message: string;
};

export type QaAccountStatus = {
  apiKey?: string | null;
  isAnonymous: boolean;
  userId?: string | null;
  userName?: string | null;
  permissionLevel: string;
  canOverwrite: boolean;
};

export type QaSearchResultItem = {
  questionId: string;
  tid?: string;
  cid?: string;
  stem: string;
  questionType: string;
  similarity: number;
  answers: string[];
  options?: { id: string; text: string }[];
  source: string;
};

const typeBadgeStyles: Record<string, string> = {
  single:
    "bg-blue-100 text-blue-800 dark:bg-blue-900/60 dark:text-blue-200 border border-blue-200 dark:border-blue-800",
  multiple:
    "bg-purple-100 text-purple-800 dark:bg-purple-900/60 dark:text-purple-200 border border-purple-200 dark:border-purple-800",
  judge:
    "bg-orange-100 text-orange-800 dark:bg-orange-900/60 dark:text-orange-200 border border-orange-200 dark:border-orange-800",
  text: "bg-green-100 text-green-800 dark:bg-green-900/60 dark:text-green-200 border border-green-200 dark:border-green-800",
};

export const typeLabels: Record<string, string> = {
  single: "单选题",
  multiple: "多选题",
  text: "填空题",
  judge: "判断题",
};

function isOptionSelected(
  values: unknown[],
  optId: string,
  optText: string,
  qType?: string,
): boolean {
  if (!Array.isArray(values)) return false;
  const optIdLower = optId.toLowerCase();
  const optTextTrim = optText.trim();
  const cleanOpt = stripHtml(optText);
  return values.some((val) => {
    if (typeof val === "boolean" || qType === "judge") {
      if (
        (val === true || val === "true") &&
        (optIdLower === "a" ||
          optTextTrim.includes("正确") ||
          optTextTrim.includes("对") ||
          cleanOpt.includes("正确") ||
          cleanOpt.includes("对"))
      )
        return true;
      if (
        (val === false || val === "false") &&
        (optIdLower === "b" ||
          optTextTrim.includes("错误") ||
          optTextTrim.includes("错") ||
          cleanOpt.includes("错误") ||
          cleanOpt.includes("错"))
      )
        return true;
    }
    const valStr = String(val).trim();
    if (valStr.toLowerCase() === optIdLower) return true;
    const cleanVal = stripHtml(valStr);
    if (
      valStr === optTextTrim ||
      (cleanOpt.length > 0 && cleanOpt === cleanVal) ||
      optTextTrim.replace(/\s+/g, "").includes(valStr.replace(/\s+/g, "")) ||
      (cleanOpt.length > 0 && cleanOpt.replace(/\s+/g, "").includes(cleanVal.replace(/\s+/g, "")))
    )
      return true;
    return false;
  });
}

function formatAnswerValues(
  values: unknown[],
  options?: { id: string; text: string }[] | null,
  qType?: string,
): string {
  if (!Array.isArray(values)) return String(values || "");
  if (qType === "text" && values.length > 1) {
    return values.map((v, i) => `第 ${i + 1} 空: ${String(v)}`).join("; ");
  }
  return values
    .map((v) => {
      if (typeof v === "boolean" || qType === "judge") {
        if (v === true || v === "true" || v === "正确" || v === "对" || v === "A") return "A. 正确";
        if (v === false || v === "false" || v === "错误" || v === "错" || v === "B")
          return "B. 错误";
      }
      const valStr = String(v).trim();
      const cleanVal = stripHtml(valStr);
      const matchedOpt = options?.find((o) => {
        if (o.id === valStr || o.id.toLowerCase() === valStr.toLowerCase()) return true;
        if (o.text.trim() === valStr) return true;
        const cleanOpt = stripHtml(o.text);
        if (cleanOpt.length > 0 && cleanOpt === cleanVal) return true;
        if (
          o.text.replace(/\s+/g, "").includes(valStr.replace(/\s+/g, "")) ||
          valStr.replace(/\s+/g, "").includes(o.text.replace(/\s+/g, ""))
        )
          return true;
        if (
          cleanOpt.length > 0 &&
          cleanVal.length > 0 &&
          (cleanOpt.replace(/\s+/g, "").includes(cleanVal.replace(/\s+/g, "")) ||
            cleanVal.replace(/\s+/g, "").includes(cleanOpt.replace(/\s+/g, "")))
        )
          return true;
        return false;
      });
      if (matchedOpt) {
        return matchedOpt.text
          ? `${matchedOpt.id.toUpperCase()}. ${matchedOpt.text}`
          : matchedOpt.id.toUpperCase();
      }
      return valStr;
    })
    .join(", ");
}

// 解析并格式化标准答案，使其显式包含真实答案选项字母 (A、B、C、D等) 和选项文本内容
export function formatCorrectAnswers(
  q: LocalBankQuestion,
): { key?: string; text: string; full: string }[] {
  if (!q.correctAnswers || q.correctAnswers.length === 0) {
    return [];
  }

  return q.correctAnswers.map((ans) => {
    const trimmed = ans.trim();
    if (!q.options || q.options.length === 0) {
      return { text: trimmed, full: trimmed };
    }

    // 1. 如果 ans 已经是选项字母 Key (如 "A", "B", "C")
    const byId = q.options.find(
      (o) =>
        o.id === trimmed || o.id.toLowerCase() === trimmed.toLowerCase() || o.id.trim() === trimmed,
    );
    if (byId) {
      return {
        key: byId.id,
        text: byId.text,
        full: byId.text ? `${byId.id}. ${byId.text}` : byId.id,
      };
    }

    // 2. 如果 ans 存放的是文本内容，从 options 中反查其当前真实选项字母
    const cleanAns = stripHtml(trimmed);
    const byText = q.options.find((o) => {
      if (o.text.trim() === trimmed) return true;
      const cleanOpt = stripHtml(o.text);
      if (cleanOpt.length > 0 && cleanOpt === cleanAns) return true;
      if (
        o.text.replace(/\s+/g, "").includes(trimmed.replace(/\s+/g, "")) ||
        trimmed.replace(/\s+/g, "").includes(o.text.replace(/\s+/g, ""))
      )
        return true;
      if (
        cleanOpt.length > 0 &&
        cleanAns.length > 0 &&
        (cleanOpt.replace(/\s+/g, "").includes(cleanAns.replace(/\s+/g, "")) ||
          cleanAns.replace(/\s+/g, "").includes(cleanOpt.replace(/\s+/g, "")))
      )
        return true;
      return false;
    });
    if (byText) {
      return {
        key: byText.id,
        text: byText.text,
        full: `${byText.id}. ${byText.text}`,
      };
    }

    return { text: trimmed, full: trimmed };
  });
}

// 复用的单题卡片展示模块 (题库浏览 Modal 与题目搜索结果 Modal 共享)
export function BankQuestionCardItem({
  question,
  similarity,
  sourceOverride,
  courseTitle,
}: {
  question: LocalBankQuestion;
  similarity?: number;
  sourceOverride?: string;
  courseTitle?: string;
}) {
  const qType = question.type || question.questionType || "single";
  const answerDetails = formatCorrectAnswers(question);
  const displayAnswers =
    question.answers && question.answers.length > 0
      ? question.answers
      : question.correctAnswers && question.correctAnswers.length > 0
        ? [
            {
              values: question.correctAnswers,
              isCorrect: true,
              source: sourceOverride || "local",
              confidence: 1.0,
            },
          ]
        : [];
  const correctRecords = displayAnswers.filter((a) => a.isCorrect);
  const wrongRecords = displayAnswers.filter((a) => !a.isCorrect);

  return (
    <div
      key={`${question.tid}-${question.questionId}`}
      className="grid gap-3 rounded-xl border border-separator/50 bg-surface/50 p-4 transition-colors hover:border-separator"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <span
            className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium border ${typeBadgeStyles[qType] ?? "bg-surface text-foreground border-separator"}`}
          >
            {typeLabels[qType] ?? qType}
          </span>
          {typeof similarity === "number" && (
            <Chip size="sm" color={similarity >= 0.8 ? "success" : "warning"}>
              相似度 {(similarity * 100).toFixed(1)}%
            </Chip>
          )}
          {qType === "text" && (question.blanks || 0) > 0 && (
            <span className="text-xs text-muted font-mono">共 {question.blanks} 空</span>
          )}
          {question.uploaded ? (
            <Chip size="sm" color="success" className="flex items-center gap-1 font-medium">
              <LuCircleCheck aria-hidden="true" /> 已同步至服务端
            </Chip>
          ) : (
            <Chip size="sm" color="warning" className="flex items-center gap-1 font-medium">
              <LuClock aria-hidden="true" /> 待同步
            </Chip>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2 font-mono text-xs text-muted">
          {question.tid && <span>tid: {question.tid}</span>}
          {question.tid && <span>·</span>}
          <span>#{question.questionId}</span>
          <span>·</span>
          <span>{courseTitle || question.courseName || `cid: ${question.cid}`}</span>
        </div>
      </div>

      <p className="font-medium whitespace-pre-wrap text-foreground">
        <DimmedHtmlText text={question.stem || "（题干未录入）"} />
      </p>

      {/* 选项列表 */}
      {question.options && question.options.length > 0 && (
        <div className="grid gap-1.5 rounded-lg border border-separator/40 bg-surface/40 p-2.5 text-xs">
          <span className="font-semibold text-muted text-[11px]">选项列表</span>
          {question.options.map((opt) => {
            const isCorrect =
              correctRecords.some((a) => isOptionSelected(a.values, opt.id, opt.text, qType)) ||
              answerDetails.some((ad) => ad.key === opt.id || ad.text === opt.text);
            const isWrong = wrongRecords.some((a) =>
              isOptionSelected(a.values, opt.id, opt.text, qType),
            );

            return (
              <div
                key={opt.id}
                className={`flex items-start gap-2 rounded-lg border px-3 py-1.5 text-xs transition-colors ${
                  isCorrect
                    ? "border-green-500 bg-green-50/80 text-green-800 dark:bg-green-950/60 dark:text-green-200 font-medium"
                    : isWrong
                      ? "border-red-400 bg-red-50/70 text-red-700 line-through dark:bg-red-950/50 dark:text-red-300"
                      : "border-separator/40 bg-surface/30 text-foreground/90"
                }`}
              >
                <span className="font-bold shrink-0">{opt.id}.</span>
                <span className="flex-1">
                  <DimmedHtmlText text={opt.text} />
                </span>
                {isCorrect && (
                  <span className="ml-auto inline-flex items-center gap-1 text-[11px] font-semibold text-green-600 dark:text-green-400 shrink-0">
                    <LuCircleCheck aria-hidden="true" /> 正确项
                  </span>
                )}
                {isWrong && (
                  <span className="ml-auto inline-flex items-center gap-1 text-[11px] font-semibold text-red-500 dark:text-red-400 shrink-0">
                    ✗ 已排除
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* 答案记录卡片区 */}
      <div className="grid gap-1.5 pt-1 border-t border-separator/30 text-xs">
        <span className="font-semibold text-muted text-[11px]">答案记录</span>
        {displayAnswers.length > 0 ? (
          displayAnswers.map((a, aIdx) => (
            <div
              key={aIdx}
              className={`flex flex-col gap-1 rounded-lg border px-3 py-2 text-xs ${
                a.isCorrect
                  ? "border-green-300 bg-green-50/60 text-green-900 dark:border-green-800 dark:bg-green-950/50 dark:text-green-200"
                  : "border-red-300 bg-red-50/60 text-red-900 dark:border-red-800 dark:bg-red-950/50 dark:text-red-200"
              }`}
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-1.5 font-bold">
                  {a.isCorrect ? (
                    <span className="text-green-600 dark:text-green-400 flex items-center gap-1">
                      <LuCircleCheck aria-hidden="true" /> 标准答案：
                    </span>
                  ) : (
                    <span className="text-red-500 flex items-center gap-1">✗ 排除答案：</span>
                  )}
                  {a.isCorrect && answerDetails.length > 0 ? (
                    <div className="flex flex-wrap items-center gap-2">
                      {answerDetails.map((ad, idx) => (
                        <span
                          key={idx}
                          className="inline-flex items-center gap-1 font-medium text-foreground"
                        >
                          {ad.key && (
                            <span className="rounded bg-success/20 px-1.5 py-0.5 font-bold font-mono text-foreground text-xs">
                              {ad.key}
                            </span>
                          )}
                          <span>{ad.text}</span>
                          {idx < answerDetails.length - 1 && <span className="text-muted">，</span>}
                        </span>
                      ))}
                    </div>
                  ) : (
                    <span className="font-semibold text-foreground">
                      {formatAnswerValues(a.values, question.options, qType)}
                    </span>
                  )}
                </div>
                <span className="text-[10px] text-muted font-mono shrink-0">
                  来源: {a.source || sourceOverride || "local"} · 置信度:{" "}
                  {Math.round((a.confidence ?? 1) * 100)}%
                </span>
              </div>
            </div>
          ))
        ) : answerDetails.length > 0 ? (
          <div className="flex flex-wrap items-center gap-2 rounded-lg border border-success/30 bg-success/10 px-3 py-2 text-xs text-success-foreground dark:text-success">
            <span className="font-bold">标准答案：</span>
            <div className="flex flex-wrap items-center gap-2">
              {answerDetails.map((a, idx) => (
                <span key={idx} className="inline-flex items-center gap-1 font-medium">
                  {a.key && (
                    <span className="rounded bg-success/20 px-1.5 py-0.5 font-bold font-mono text-foreground">
                      {a.key}
                    </span>
                  )}
                  <span>{a.text}</span>
                  {idx < answerDetails.length - 1 && <span className="text-muted">，</span>}
                </span>
              ))}
            </div>
          </div>
        ) : (
          <div className="rounded-lg border border-separator/40 bg-surface/30 px-3 py-2 text-muted italic">
            <span>暂无答案记录</span>
          </div>
        )}
      </div>

      <div className="flex items-center justify-between text-[11px] text-muted pt-0.5">
        {sourceOverride && <span>检索来源：{sourceOverride}</span>}
        {question.updatedAtMs > 0 && (
          <span>更新时间：{new Date(question.updatedAtMs).toLocaleString()}</span>
        )}
      </div>
    </div>
  );
}

export { DEFAULT_QA_SERVER_URL };

export function UoocQaSettings({
  courses,
  quizzes,
  qaServerUrl = DEFAULT_QA_SERVER_URL,
  onQaServerUrlChange,
  debugEnabled = false,
  disabled = false,
  onSaveSettings,
}: {
  courses?: UoocCourse[];
  quizzes?: Record<string, QuizState>;
  qaServerUrl?: string;
  onQaServerUrlChange?: (url: string) => void;
  debugEnabled?: boolean;
  disabled?: boolean;
  onSaveSettings?: () => void;
} = {}) {
  const [vectorStatus, setVectorStatus] = useState<EmbeddingVectorStatus>();
  const [accountStatus, setAccountStatus] = useState<QaAccountStatus>();
  const [loading, setLoading] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchResults, setSearchResults] = useState<QaSearchResultItem[]>([]);
  const [searchModalOpen, setSearchModalOpen] = useState(false);
  const [customApiKey, setCustomApiKey] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [deviceAuthInfo, setDeviceAuthInfo] = useState<{
    userCode: string;
    verificationUriComplete: string;
    deviceCode: string;
    interval: number;
  } | null>(null);
  const [deviceAuthPolling, setDeviceAuthPolling] = useState(false);
  const [fullUploading, setFullUploading] = useState(false);

  // 本地题库状态
  const [localQuestions, setLocalQuestions] = useState<LocalBankQuestion[]>([]);
  const [loadingQuestions, setLoadingQuestions] = useState(false);
  const [rebuildingBank, setRebuildingBank] = useState(false);
  const [rebuildModalOpen, setRebuildModalOpen] = useState(false);
  const [clearingBank, setClearingBank] = useState(false);
  const [confirmClearBank, setConfirmClearBank] = useState(false);
  const [courseTitles, setCourseTitles] = useState<Record<string, string>>({});
  const [showHtmlTags, setShowHtmlTags] = useHtmlTagVisibility();

  // 弹窗浏览状态 ("all" 表示全部课程，具体数字表示指定 cid)
  const [modalCourse, setModalCourse] = useState<string | null>(null);
  const [bankTidFilter, setBankTidFilter] = useState<string>("all");
  const [bankSearchQuery, setBankSearchQuery] = useState("");
  const [bankStatusFilter, setBankStatusFilter] = useState<"all" | "synced" | "pending">("all");
  const [bankTypeFilter, setBankTypeFilter] = useState<
    "all" | "single" | "multiple" | "judge" | "text"
  >("all");
  const [bankPage, setBankPage] = useState(1);
  const pageSize = 10;

  const refreshStatus = async () => {
    setLoading(true);
    try {
      const [vStatus, aStatus] = await Promise.all([
        invoke<EmbeddingVectorStatus>("get_uooc_embedding_status"),
        invoke<QaAccountStatus>("get_uooc_qa_account_status", {
          serverUrl: qaServerUrl,
        }).catch(() => undefined),
      ]);
      setVectorStatus(vStatus);
      if (aStatus) {
        setAccountStatus((prev) => ({
          ...aStatus,
          apiKey: aStatus.apiKey || prev?.apiKey || customApiKey.trim() || undefined,
        }));
        if (aStatus.apiKey) setCustomApiKey(aStatus.apiKey);
      }
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "无法获取题库状态";
      toast.danger("无法获取题库状态", { description: msg });
    } finally {
      setLoading(false);
    }
  };

  const refreshLocalQuestions = async () => {
    setLoadingQuestions(true);
    try {
      const list = await invoke<LocalBankQuestion[]>("get_uooc_local_bank_questions");
      setLocalQuestions(list || []);
    } catch {
      // 在未初始化的测试中静默忽略
    } finally {
      setLoadingQuestions(false);
    }
  };

  useEffect(() => {
    void refreshStatus();
  }, [qaServerUrl]);

  useEffect(() => {
    const onFocus = () => {
      if (!accountStatus?.canOverwrite) {
        void refreshStatus();
      }
    };
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
    };
  }, [qaServerUrl, accountStatus?.canOverwrite]);

  useEffect(() => {
    void refreshLocalQuestions();
  }, []);

  // 异步通过 UOOC 服务获取未知课程 ID 的真实课程名称
  useEffect(() => {
    if (localQuestions.length === 0) return;
    const cids = Array.from(new Set(localQuestions.map((q) => q.cid)));
    for (const cid of cids) {
      if (!courseTitles[cid]) {
        const found = courses?.find((c) => c.id === cid);
        if (found) {
          setCourseTitles((prev) => ({ ...prev, [cid]: found.title }));
        } else {
          void invoke<string>("get_uooc_course_title", { cid })
            .then((title) => {
              if (title) {
                setCourseTitles((prev) => ({ ...prev, [cid]: title }));
              }
            })
            .catch(() => {});
        }
      }
    }
  }, [localQuestions, courses]);

  const handleDownload = async () => {
    setDownloading(true);
    try {
      const next = await invoke<EmbeddingVectorStatus>("download_uooc_qa_vectors", {
        serverUrl: qaServerUrl,
      });
      setVectorStatus(next);
      toast.success(`题库向量下载完成，已缓存 ${next.cachedCount} 条题目向量`);
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "下载题库向量失败";
      toast.danger("下载题库向量失败", { description: msg });
    } finally {
      setDownloading(false);
    }
  };

  const handleSyncBank = async () => {
    setSyncing(true);
    try {
      const count = await invoke<number>("sync_uooc_qa_bank", {
        serverUrl: qaServerUrl,
      });
      toast.success(
        count > 0
          ? `已成功与后端比对并将 ${count} 道缺失题目同步上传`
          : "题目已与服务端保持最新同步",
      );
      await refreshLocalQuestions();
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "同步题库失败";
      toast.danger("同步题库失败", { description: msg });
    } finally {
      setSyncing(false);
    }
  };

  const handleSearch = async () => {
    const trimmed = searchQuery.trim();
    if (!trimmed) return;
    setSearching(true);
    try {
      const results = await invoke<QaSearchResultItem[]>("search_uooc_qa_questions", {
        query: trimmed,
        serverUrl: qaServerUrl,
      });
      setSearchResults(results);
      setSearchModalOpen(true);
      if (results.length === 0) {
        toast.warning("未找到匹配题目");
      }
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "向量检索失败";
      toast.danger("向量检索失败", { description: msg });
    } finally {
      setSearching(false);
    }
  };

  const handleSaveApiKey = async () => {
    setSavingKey(true);
    try {
      await invoke("save_uooc_qa_api_key", { apiKey: customApiKey.trim() });
      toast.success("题库 API Key 保存成功");
      await refreshStatus();
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "保存 API Key 失败";
      toast.danger("保存 API Key 失败", { description: msg });
    } finally {
      setSavingKey(false);
    }
  };

  const handleResetApiKey = async () => {
    setSavingKey(true);
    try {
      await invoke("reset_uooc_qa_api_key");
      setCustomApiKey("");
      toast.success("题库凭据已重置");
      await refreshStatus();
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "重置凭据失败";
      toast.danger("重置凭据失败", { description: msg });
    } finally {
      setSavingKey(false);
    }
  };

  const handleStartDeviceAuth = async () => {
    setDeviceAuthPolling(true);
    try {
      const client = getQaAuthClient(qaServerUrl);
      const res = await client.device.code({
        client_id: "your-szu-life",
      });

      if (res.error || !res.data) {
        const errorDesc =
          res.error?.error_description ||
          res.error?.error ||
          "申请设备码失败，请确认题库服务正常运行";
        throw new Error(errorDesc);
      }

      const rawComplete =
        res.data.verification_uri_complete || res.data.verification_uri || "/device";
      const normalizedBase = normalizeQaBaseUrl(qaServerUrl);
      const fullCompleteUrl = rawComplete.startsWith("http")
        ? rawComplete
        : `${normalizedBase}${rawComplete.startsWith("/") ? "" : "/"}${rawComplete}`;

      setDeviceAuthInfo({
        userCode: res.data.user_code,
        verificationUriComplete: fullCompleteUrl,
        deviceCode: res.data.device_code,
        interval: res.data.interval || 5,
      });
      void openPublicUrl(fullCompleteUrl);
    } catch (e: unknown) {
      const err = e as { message?: string } | string;
      const msg =
        typeof err === "string"
          ? err
          : err?.message || "发起设备码授权失败，请检查题库服务地址与连接";
      toast.danger("发起设备码授权失败", { description: msg });
      setDeviceAuthPolling(false);
    }
  };

  const handleCancelDeviceAuth = () => {
    setDeviceAuthInfo(null);
    setDeviceAuthPolling(false);
  };

  useEffect(() => {
    if (!deviceAuthInfo) return;
    let active = true;

    const poll = async () => {
      try {
        const client = getQaAuthClient(qaServerUrl);
        const tokenRes = await client.device.token({
          grant_type: "urn:ietf:params:oauth:grant-type:device_code",
          device_code: deviceAuthInfo.deviceCode,
          client_id: "your-szu-life",
        });

        if (!active) return;

        if (tokenRes.error) {
          const errCode = tokenRes.error.error;
          if (errCode === "authorization_pending" || errCode === "slow_down") {
            return;
          }
          setDeviceAuthInfo(null);
          setDeviceAuthPolling(false);
          const desc = tokenRes.error.error_description || `授权失败: ${errCode}`;
          toast.danger("设备授权未完成", { description: desc });
          return;
        }

        const accessToken = tokenRes.data?.access_token;
        if (!accessToken) {
          return;
        }

        const authedClient = getQaAuthClient(qaServerUrl, accessToken);
        const keyRes = await authedClient.apiKey.create({
          name: "your-szu-life",
          prefix: "uooc_",
          expiresIn: 60 * 60 * 24 * 365,
        });

        const apiKey = keyRes.data?.key || (keyRes.data as { apiKey?: string } | undefined)?.apiKey;
        if (!apiKey) {
          throw new Error(keyRes.error?.message || "设备已批准，但创建专属 API Key 失败");
        }

        await invoke("save_uooc_qa_api_key", { apiKey });
        setDeviceAuthInfo(null);
        setDeviceAuthPolling(false);
        setCustomApiKey(apiKey);
        toast.success("设备码授权登录成功！已自动获取并绑定专属 API Key");
        await refreshStatus();
      } catch (e: unknown) {
        if (!active) return;
        const err = e as { message?: string } | string;
        const msg = typeof err === "string" ? err : err?.message || "轮询设备授权失败";
        setDeviceAuthInfo(null);
        setDeviceAuthPolling(false);
        toast.danger("轮询设备授权失败", { description: msg });
      }
    };

    const intervalMs = (deviceAuthInfo.interval || 5) * 1000;
    const timer = setInterval(() => {
      void poll();
    }, intervalMs);

    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [deviceAuthInfo, qaServerUrl]);

  const handleFullUpload = async () => {
    setFullUploading(true);
    try {
      const count = await invoke<number>("upload_all_uooc_qa_bank", {
        serverUrl: qaServerUrl,
      });
      toast.success(
        count > 0 ? `已成功全量上传 ${count} 道题目至题库共享平台` : "题库中暂无题目可上传",
      );
      await refreshLocalQuestions();
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "全量上传题库失败";
      toast.danger("全量上传题库失败", { description: msg });
    } finally {
      setFullUploading(false);
    }
  };

  const handleRebuildBank = async () => {
    setRebuildingBank(true);
    setRebuildModalOpen(true);
    try {
      const count = await invoke<number>("build_uooc_local_bank");
      toast.success(`题库已重新构建完成，共收录 ${count} 道题目`);
      await refreshLocalQuestions();
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "构建题库失败";
      toast.danger("构建题库失败", { description: msg });
    } finally {
      setRebuildingBank(false);
    }
  };

  const handleOpenLogin = async () => {
    const base = (qaServerUrl || DEFAULT_QA_SERVER_URL).replace(/\/+$/, "");
    let key = accountStatus?.apiKey || customApiKey.trim() || undefined;

    if (!key) {
      try {
        const next = await invoke<QaAccountStatus>("get_uooc_qa_account_status", {
          serverUrl: qaServerUrl,
        });
        if (next?.apiKey) {
          key = next.apiKey;
          setAccountStatus((prev) => ({
            ...(prev || next),
            ...next,
            apiKey: next.apiKey,
          }));
          setCustomApiKey(next.apiKey);
        }
      } catch {
        // 忽略网络或调用异常
      }
    }

    if (!key) {
      toast.warning(
        "未能自动获取到题库凭据，已直接打开登录页面。若需认证请确认题库服务正常后重试。",
      );
    }

    const keyParam = key ? `?apiKey=${encodeURIComponent(key)}` : "";
    const loginUrl = `${base}/login${keyParam}`;
    void openPublicUrl(loginUrl);
  };

  // 全量汇总计数
  const uploadedCount = useMemo(
    () => localQuestions.filter((q) => q.uploaded).length,
    [localQuestions],
  );
  const pendingCount = localQuestions.length - uploadedCount;

  // 按课程分组 (cid 分组)，课程名称通过 UOOC 服务、本地快照或历史字段解析
  const courseGroups = useMemo(() => {
    const map = new Map<string, LocalBankQuestion[]>();
    for (const q of localQuestions) {
      const list = map.get(q.cid) || [];
      list.push(q);
      map.set(q.cid, list);
    }
    return Array.from(map.entries()).map(([cid, questions]) => {
      const uploaded = questions.filter((q) => q.uploaded).length;
      const title =
        courseTitles[cid] ||
        courses?.find((c) => c.id === cid)?.title ||
        questions.find((q) => q.courseName)?.courseName ||
        `课程 ${cid}`;
      return {
        cid,
        title,
        questions,
        total: questions.length,
        uploaded,
        pending: questions.length - uploaded,
      };
    });
  }, [localQuestions, courseTitles, courses]);

  // Modal 中当前展示的试题集与筛选逻辑
  const activeCourseQuestions = useMemo(() => {
    if (!modalCourse) return [];
    if (modalCourse === "all") return localQuestions;
    return localQuestions.filter((q) => q.cid === modalCourse);
  }, [localQuestions, modalCourse]);

  const activeCourseUploaded = useMemo(
    () => activeCourseQuestions.filter((q) => q.uploaded).length,
    [activeCourseQuestions],
  );
  const activeCoursePending = activeCourseQuestions.length - activeCourseUploaded;

  // 提取当前课程或全部课程下包含的所有题单 (tid)
  const availableTids = useMemo(() => {
    const set = new Set<string>();
    for (const q of activeCourseQuestions) {
      if (q.tid) set.add(q.tid);
    }
    return Array.from(set);
  }, [activeCourseQuestions]);

  const tidLabels = useMemo(() => {
    const map: Record<string, string> = {};
    for (const tid of availableTids) {
      let title = "";
      if (quizzes) {
        for (const q of Object.values(quizzes)) {
          if (q.taskId === tid && q.title) {
            title = q.title;
            break;
          }
        }
      }
      const count = activeCourseQuestions.filter((q) => q.tid === tid).length;
      map[tid] = title ? `${title} (${count}题)` : `题单 ${tid} (${count}题)`;
    }
    return map;
  }, [availableTids, quizzes, activeCourseQuestions]);

  const filteredQuestions = useMemo(() => {
    let list = activeCourseQuestions;
    if (bankTidFilter !== "all") {
      list = list.filter((q) => q.tid === bankTidFilter);
    }
    if (bankStatusFilter === "synced") {
      list = list.filter((q) => q.uploaded);
    } else if (bankStatusFilter === "pending") {
      list = list.filter((q) => !q.uploaded);
    }
    if (bankTypeFilter !== "all") {
      list = list.filter((q) => q.questionType === bankTypeFilter);
    }
    const qText = bankSearchQuery.trim().toLowerCase();
    if (qText) {
      list = list.filter(
        (item) =>
          (item.stem && item.stem.toLowerCase().includes(qText)) ||
          item.questionId.toLowerCase().includes(qText) ||
          item.tid.toLowerCase().includes(qText) ||
          item.cid.toLowerCase().includes(qText) ||
          (item.correctAnswers &&
            item.correctAnswers.some((a) => a.toLowerCase().includes(qText))) ||
          (item.answers &&
            item.answers.some((a) =>
              (a.values || []).some((v) => String(v).toLowerCase().includes(qText)),
            )),
      );
    }
    return list;
  }, [activeCourseQuestions, bankTidFilter, bankStatusFilter, bankTypeFilter, bankSearchQuery]);

  const totalItems = filteredQuestions.length;
  const totalPages = Math.max(1, Math.ceil(totalItems / pageSize));
  const safePage = Math.min(bankPage, totalPages);
  const startItem = totalItems === 0 ? 0 : (safePage - 1) * pageSize + 1;
  const endItem = Math.min(safePage * pageSize, totalItems);
  const paginatedQuestions = useMemo(
    () => filteredQuestions.slice((safePage - 1) * pageSize, safePage * pageSize),
    [filteredQuestions, safePage, pageSize],
  );

  const handleOpenCourseModal = (target: string) => {
    setModalCourse(target);
    setBankTidFilter("all");
    setBankSearchQuery("");
    setBankStatusFilter("all");
    setBankTypeFilter("all");
    setBankPage(1);
  };

  // 将搜索结果转换为通用的题目对象，优先关联本地已有完整题单详情
  const convertedSearchResults = useMemo(() => {
    return searchResults.map((item) => {
      const matchedLocal = localQuestions.find(
        (lq) =>
          lq.questionId === item.questionId ||
          (item.tid && lq.tid === item.tid && lq.questionId === item.questionId),
      );
      if (matchedLocal) {
        return {
          question: matchedLocal,
          similarity: item.similarity,
          source: item.source,
        };
      }
      const q: LocalBankQuestion = {
        questionId: item.questionId,
        tid: item.tid || "",
        cid: item.cid || "",
        courseName: item.cid ? courseTitles[item.cid] : undefined,
        questionType: item.questionType,
        stem: item.stem,
        options: item.options?.map((o) => ({ id: o.id, text: o.text })),
        correctAnswers: item.answers,
        answers: item.answers.map((a) => ({
          values: [a],
          isCorrect: true,
          source: item.source,
          confidence: item.similarity,
        })),
        uploaded: true,
        updatedAtMs: 0,
      };
      return {
        question: q,
        similarity: item.similarity,
        source: item.source,
      };
    });
  }, [searchResults, localQuestions, courseTitles]);

  return (
    <div className="grid gap-6">
      {/* 题库共享平台服务与认证配置 */}
      <Card id="uooc-qa-share-settings" aria-labelledby="uooc-qa-share-title">
        <Card.Header>
          <Card.Title id="uooc-qa-share-title">题库服务与账户配置</Card.Title>
          <Card.Description>
            管理题库共享平台的连接地址、账户登录、向量同步与全量试题上传。做题前可使用题库答案直接作答或排除错误选项，答题完成后自动回传标准答案。
          </Card.Description>
        </Card.Header>
        <Card.Content className="grid gap-5">
          {loading && !vectorStatus && !accountStatus && (
            <div className="flex items-center gap-2 text-sm text-muted" role="status">
              <Spinner size="sm" /> 正在检查题库状态
            </div>
          )}

          {/* 题库共享后端地址配置框 */}
          <div className="grid gap-3 rounded-xl border border-separator/60 p-4">
            <div className="flex items-center gap-2">
              {debugEnabled && (
                <Chip color="warning" size="sm">
                  调试选项
                </Chip>
              )}
              <Label htmlFor="uooc-qa-server-url">题库共享后端地址 (uooc-qa-share)</Label>
            </div>
            <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
              <div className="flex-1">
                <TextField
                  id="uooc-qa-server-url"
                  value={qaServerUrl ?? DEFAULT_QA_SERVER_URL}
                  isDisabled={disabled}
                  onChange={onQaServerUrlChange}
                >
                  <Input placeholder={DEFAULT_QA_SERVER_URL} />
                  <Description>
                    修改题库共享平台的后端服务地址 (RFC 8628 设备认证与上传接口)
                  </Description>
                </TextField>
              </div>
              {onSaveSettings && (
                <Button variant="secondary" isDisabled={disabled} onPress={onSaveSettings}>
                  保存题库地址
                </Button>
              )}
            </div>
          </div>

          {/* 账户与认证登录状态卡片 */}
          <div className="grid gap-3 rounded-xl border border-separator/60 p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2 font-medium">
                <LuUser aria-hidden="true" />
                题库账户状态
              </div>
              {accountStatus?.canOverwrite ? (
                <Chip size="sm" color="success" className="flex items-center gap-1">
                  <LuShieldCheck aria-hidden="true" /> 认证用户（读+写）
                </Chip>
              ) : (
                <Chip size="sm" color="warning" className="flex items-center gap-1">
                  <LuShieldAlert aria-hidden="true" /> 匿名账户（读+新增）
                </Chip>
              )}
            </div>

            <p className="text-xs text-muted">
              {accountStatus?.canOverwrite
                ? `已登录为认证用户 (${accountStatus.userName || accountStatus.userId || "已授权"})，享有试题答案修改与覆盖更新权限。`
                : "当前使用自动签发的匿名账户。可在题库网页绑定 GitHub 或 bili-sign 账号升级为认证用户，原有答案与凭据将无缝继承。"}
            </p>

            <div className="flex flex-wrap items-center gap-2 pt-1">
              <Button
                variant="primary"
                isDisabled={disabled}
                onPress={() => void handleOpenLogin()}
              >
                <LuExternalLink aria-hidden="true" />
                {accountStatus?.canOverwrite ? "个人中心" : "认证账号"}
              </Button>
              <Button
                variant="secondary"
                isDisabled={disabled || Boolean(deviceAuthInfo)}
                onPress={() => void handleStartDeviceAuth()}
              >
                {deviceAuthPolling && !deviceAuthInfo ? (
                  <Spinner size="sm" />
                ) : (
                  <LuLaptop aria-hidden="true" />
                )}
                通过设备码登录
              </Button>
              {accountStatus?.apiKey && (
                <Button
                  variant="tertiary"
                  isDisabled={disabled || savingKey}
                  onPress={() => void handleResetApiKey()}
                >
                  重置凭据
                </Button>
              )}
            </div>

            {deviceAuthInfo && (
              <div className="mt-2 grid gap-3 rounded-xl border border-primary/40 bg-primary/5 p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2 font-medium text-foreground">
                    <LuLaptop aria-hidden="true" />
                    设备码授权认证中 (RFC 8628)
                  </div>
                  <Chip size="sm" color="accent" className="flex items-center gap-1">
                    <Spinner size="sm" /> 等待浏览器批准
                  </Chip>
                </div>
                <p className="text-xs text-muted">
                  请在打开的浏览器页面中确认授权。若页面未自动打开，可点击下方按钮在浏览器中打开：
                </p>
                <div className="flex items-center gap-3 py-1">
                  <span className="text-sm text-muted">设备验证码：</span>
                  <code className="rounded-lg border border-separator/60 bg-surface px-3 py-1 text-base font-bold tracking-widest text-primary">
                    {deviceAuthInfo.userCode}
                  </code>
                </div>
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  <Button
                    variant="primary"
                    size="sm"
                    onPress={() => void openPublicUrl(deviceAuthInfo.verificationUriComplete)}
                  >
                    <LuExternalLink aria-hidden="true" />
                    在浏览器中打开授权页
                  </Button>
                  <Button variant="tertiary" size="sm" onPress={handleCancelDeviceAuth}>
                    取消授权
                  </Button>
                </div>
              </div>
            )}

            {/* 手动配置 API Key */}
            <div className="mt-2 border-t border-separator/40 pt-3">
              <Label className="text-xs font-medium text-muted">手动配置 API Key</Label>
              <div className="mt-1 flex flex-col gap-2 sm:flex-row">
                <div className="flex-1">
                  <TextField
                    aria-label="题库 API Key"
                    value={customApiKey}
                    isDisabled={disabled || savingKey}
                    onChange={setCustomApiKey}
                  >
                    <Input placeholder="粘贴 uooc_ 开头的专属 API Key..." />
                    <Description>
                      若在其他设备或网页端已生成专属 API Key，可直接填入复用已认证身份
                    </Description>
                  </TextField>
                </div>
                <Button
                  className="self-start sm:self-auto"
                  variant="secondary"
                  isDisabled={disabled || savingKey || !customApiKey.trim()}
                  onPress={() => void handleSaveApiKey()}
                >
                  {savingKey ? <Spinner size="sm" /> : <LuKey aria-hidden="true" />}
                  保存 Key
                </Button>
              </div>
            </div>
          </div>

          {/* 向量库状态与同步操作 */}
          {vectorStatus && (
            <Alert status={vectorStatus.cachedCount > 0 ? "success" : "default"}>
              <Alert.Content>
                <Alert.Title className="flex items-center gap-2">
                  {vectorStatus.cachedCount > 0 ? (
                    <>
                      <LuCircleCheck aria-hidden="true" /> 题库向量库已就绪
                    </>
                  ) : (
                    "尚未下载题库向量"
                  )}
                </Alert.Title>
                <Alert.Description>
                  {vectorStatus.cachedCount > 0
                    ? `已缓存 ${vectorStatus.cachedCount} 条题目向量 · ${vectorStatus.totalDimension} 维浮点嵌入`
                    : "点击下方按钮下载题库标记向量，下载后可高速检索或向服务端免计算搜题"}
                </Alert.Description>
              </Alert.Content>
            </Alert>
          )}

          <div className="flex flex-wrap items-center gap-3">
            <Button
              variant="primary"
              isDisabled={downloading || syncing || fullUploading}
              onPress={() => void handleDownload()}
            >
              {downloading ? <Spinner size="sm" /> : <LuDownload aria-hidden="true" />}
              {vectorStatus && vectorStatus.cachedCount > 0
                ? "更新题库向量"
                : "下载 UOOC 标记题库向量"}
            </Button>

            <Button
              variant="secondary"
              isDisabled={downloading || syncing || fullUploading}
              onPress={() => void handleSyncBank()}
            >
              {syncing ? <Spinner size="sm" /> : <LuUpload aria-hidden="true" />}
              同步已确认题目至服务端
            </Button>

            <Button
              variant="secondary"
              isDisabled={downloading || syncing || fullUploading}
              onPress={() => void handleFullUpload()}
            >
              {fullUploading ? <Spinner size="sm" /> : <LuUpload aria-hidden="true" />}
              手动全题目上传
            </Button>

            <Button
              variant="tertiary"
              isDisabled={loading || downloading || syncing || fullUploading}
              onPress={() => void refreshStatus()}
            >
              <LuRefreshCw aria-hidden="true" />
              刷新状态
            </Button>
          </div>
        </Card.Content>
      </Card>

      {/* 题库内容按课程展开卡片 */}
      <Card id="uooc-qa-bank-browser" aria-labelledby="uooc-qa-bank-title">
        <Card.Header>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <Card.Title id="uooc-qa-bank-title">题库浏览</Card.Title>
              <Card.Description>
                题库按课程归类展示。点击任意课程或【全部课程】可在弹窗中进行详细的单题过滤浏览。
              </Card.Description>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                variant="secondary"
                size="sm"
                isDisabled={rebuildingBank || loadingQuestions}
                onPress={() => void handleRebuildBank()}
              >
                {rebuildingBank ? <Spinner size="sm" /> : <LuRefreshCw aria-hidden="true" />}
                重新构建题库
              </Button>
              <Button
                variant="tertiary"
                size="sm"
                isDisabled={loadingQuestions}
                onPress={() => void refreshLocalQuestions()}
              >
                <LuRefreshCw aria-hidden="true" />
                刷新列表
              </Button>
              {debugEnabled && (
                <Button
                  variant="tertiary"
                  size="sm"
                  className="text-danger hover:text-danger hover:bg-danger/10"
                  isDisabled={clearingBank || loadingQuestions || localQuestions.length === 0}
                  onPress={() => setConfirmClearBank(true)}
                >
                  {clearingBank ? <Spinner size="sm" /> : <LuTrash2 aria-hidden="true" />}
                  清空题库缓存
                </Button>
              )}
            </div>
          </div>
        </Card.Header>
        <Card.Content className="grid gap-4">
          {/* 全部课程入口卡片 */}
          <div
            className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/40 bg-primary/5 p-4 transition-colors hover:border-primary cursor-pointer"
            onClick={() => handleOpenCourseModal("all")}
          >
            <div className="flex items-center gap-3">
              <div className="rounded-lg bg-primary/10 p-2 text-primary">
                <LuLayers className="size-5" aria-hidden="true" />
              </div>
              <div>
                <h4 className="font-semibold text-foreground text-base">全部课程</h4>
                <p className="text-xs text-muted">汇总浏览题库中收录的全部课程试题</p>
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Chip size="sm" color="default">
                全部收录: {localQuestions.length} 道
              </Chip>
              <Chip size="sm" color="success">
                已同步至服务端: {uploadedCount} 道
              </Chip>
              {pendingCount > 0 && (
                <Chip size="sm" color="warning">
                  待同步: {pendingCount} 道
                </Chip>
              )}
              <Button variant="primary" size="sm" onPress={() => handleOpenCourseModal("all")}>
                浏览全部题目
              </Button>
            </div>
          </div>

          {/* 各课程卡片网格 */}
          {loadingQuestions ? (
            <div className="flex items-center justify-center gap-2 py-8 text-sm text-muted">
              <Spinner size="sm" /> 正在读取题库...
            </div>
          ) : courseGroups.length === 0 ? (
            <div className="rounded-xl border border-dashed border-separator/60 p-8 text-center text-sm text-muted">
              题库暂无已记录的题目。进入测验或点击“重新构建题库”可自动解析已完成测验的标准答案。
            </div>
          ) : (
            <div className="grid gap-3.5 sm:grid-cols-2">
              {courseGroups.map((group) => (
                <div
                  key={group.cid}
                  className="flex flex-col justify-between gap-3 rounded-xl border border-separator/60 bg-surface/40 p-4 transition-colors hover:border-separator cursor-pointer"
                  onClick={() => handleOpenCourseModal(group.cid)}
                >
                  <div className="grid gap-1">
                    <div className="flex items-start justify-between gap-2">
                      <h4 className="font-semibold text-foreground text-sm line-clamp-2">
                        {group.title}
                      </h4>
                      <Chip size="sm" color="accent" className="shrink-0 font-mono">
                        cid: {group.cid}
                      </Chip>
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-separator/30">
                    <div className="flex flex-wrap items-center gap-1.5 text-xs">
                      <Chip size="sm" color="default">
                        共 {group.total} 题
                      </Chip>
                      <Chip size="sm" color="success">
                        已同步 {group.uploaded}
                      </Chip>
                      {group.pending > 0 && (
                        <Chip size="sm" color="warning">
                          待同步 {group.pending}
                        </Chip>
                      )}
                    </div>
                    <Button
                      variant="secondary"
                      size="sm"
                      onPress={() => handleOpenCourseModal(group.cid)}
                    >
                      查看题目
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </Card.Content>
      </Card>

      {/* 课程试题单题过滤浏览 Modal */}
      <Modal
        isOpen={modalCourse !== null}
        onOpenChange={(open) => {
          if (!open) {
            setModalCourse(null);
            setBankSearchQuery("");
            setBankStatusFilter("all");
            setBankTypeFilter("all");
            setBankPage(1);
          }
        }}
      >
        <Modal.Backdrop>
          <Modal.Container size="cover" className="max-w-4xl">
            <Modal.Dialog
              aria-label="题库题目浏览"
              className="w-full max-w-4xl min-w-0 overflow-hidden"
            >
              <Modal.CloseTrigger aria-label="关闭" />
              <Modal.Header className="flex flex-wrap items-center justify-between gap-3 pr-10">
                <div className="grid gap-1">
                  <Modal.Heading>
                    {modalCourse === "all"
                      ? "全部课程题库"
                      : `${courseTitles[modalCourse || ""] || courses?.find((c) => c.id === modalCourse)?.title || `课程 ${modalCourse}`}`}
                  </Modal.Heading>
                  <p className="text-xs text-muted">
                    {modalCourse === "all"
                      ? `已收录 ${localQuestions.length} 道题目 · 已同步 ${uploadedCount} · 待同步 ${pendingCount}`
                      : `课程号 (cid): ${modalCourse} · 共 ${activeCourseQuestions.length} 道题目 · 已同步 ${activeCourseUploaded} · 待同步 ${activeCoursePending}`}
                  </p>
                </div>
                <Switch
                  size="sm"
                  isSelected={showHtmlTags}
                  onChange={setShowHtmlTags}
                  aria-label="显示 HTML 标签"
                >
                  <Switch.Content className="text-xs">
                    <Switch.Control>
                      <Switch.Thumb />
                    </Switch.Control>
                    HTML 标签
                  </Switch.Content>
                </Switch>
              </Modal.Header>
              <Modal.Body className="max-h-[75vh] overflow-y-auto grid gap-4 p-4 sm:p-6">
                {/* 搜索与筛选工具栏 */}
                <div className="grid gap-3 rounded-xl border border-separator/50 bg-surface/30 p-3.5 text-sm">
                  <TextField
                    aria-label="搜索题库内容"
                    value={bankSearchQuery}
                    onChange={(val) => {
                      setBankSearchQuery(val);
                      setBankPage(1);
                    }}
                  >
                    <Input placeholder="搜索题干关键词、题号、题组 (tid) 或答案内容..." />
                  </TextField>

                  <div className="flex flex-wrap items-center justify-between gap-3 text-xs">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="flex items-center gap-1 font-medium text-muted">
                        <LuFilter aria-hidden="true" /> 同步状态:
                      </span>
                      <div className="overflow-x-auto max-w-full">
                        <ButtonGroup size="sm" variant="tertiary" className="min-w-max">
                          <Button
                            variant={bankStatusFilter === "all" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankStatusFilter("all");
                              setBankPage(1);
                            }}
                          >
                            全部 ({activeCourseQuestions.length})
                          </Button>
                          <Button
                            variant={bankStatusFilter === "synced" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankStatusFilter("synced");
                              setBankPage(1);
                            }}
                          >
                            已同步 ({activeCourseUploaded})
                          </Button>
                          <Button
                            variant={bankStatusFilter === "pending" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankStatusFilter("pending");
                              setBankPage(1);
                            }}
                          >
                            待同步 ({activeCoursePending})
                          </Button>
                        </ButtonGroup>
                      </div>
                    </div>

                    <div className="flex flex-wrap items-center gap-2">
                      <span className="flex items-center gap-1 font-medium text-muted">
                        <LuLayers aria-hidden="true" /> 题型:
                      </span>
                      <div className="overflow-x-auto max-w-full">
                        <ButtonGroup size="sm" variant="tertiary" className="min-w-max">
                          <Button
                            variant={bankTypeFilter === "all" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankTypeFilter("all");
                              setBankPage(1);
                            }}
                          >
                            全部
                          </Button>
                          <Button
                            variant={bankTypeFilter === "single" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankTypeFilter("single");
                              setBankPage(1);
                            }}
                          >
                            单选
                          </Button>
                          <Button
                            variant={bankTypeFilter === "multiple" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankTypeFilter("multiple");
                              setBankPage(1);
                            }}
                          >
                            多选
                          </Button>
                          <Button
                            variant={bankTypeFilter === "judge" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankTypeFilter("judge");
                              setBankPage(1);
                            }}
                          >
                            判断
                          </Button>
                          <Button
                            variant={bankTypeFilter === "text" ? "primary" : "tertiary"}
                            onPress={() => {
                              setBankTypeFilter("text");
                              setBankPage(1);
                            }}
                          >
                            填空
                          </Button>
                        </ButtonGroup>
                      </div>
                    </div>
                  </div>

                  {/* 题单选择器：对于特定课程（或全部课程包含多个题单时），直接以选择的形式选择题单来查看 */}
                  {availableTids.length > 1 && (
                    <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-separator/30">
                      <span className="flex items-center gap-1 font-medium text-muted">
                        <LuFileText aria-hidden="true" /> 选择题单:
                      </span>
                      <Select
                        aria-label="选择题单"
                        value={bankTidFilter}
                        onChange={(val) => {
                          if (typeof val === "string") {
                            setBankTidFilter(val);
                            setBankPage(1);
                          }
                        }}
                        className="w-full sm:w-72"
                      >
                        <Select.Trigger>
                          <Select.Value />
                          <Select.Indicator />
                        </Select.Trigger>
                        <Select.Popover>
                          <ListBox>
                            <ListBox.Item
                              id="all"
                              textValue={`全部题单 (${activeCourseQuestions.length}题)`}
                            >
                              全部题单 ({activeCourseQuestions.length}题)
                              <ListBox.ItemIndicator />
                            </ListBox.Item>
                            {availableTids.map((tid) => (
                              <ListBox.Item
                                key={tid}
                                id={tid}
                                textValue={tidLabels[tid] || `题单 ${tid}`}
                              >
                                {tidLabels[tid] || `题单 ${tid}`}
                                <ListBox.ItemIndicator />
                              </ListBox.Item>
                            ))}
                          </ListBox>
                        </Select.Popover>
                      </Select>
                      {bankTidFilter !== "all" && (
                        <Button
                          variant="tertiary"
                          size="sm"
                          onPress={() => {
                            setBankTidFilter("all");
                            setBankPage(1);
                          }}
                        >
                          显示全部题单
                        </Button>
                      )}
                    </div>
                  )}
                </div>

                {/* 单题列表 */}
                {paginatedQuestions.length === 0 ? (
                  <div className="rounded-xl border border-dashed border-separator/60 p-8 text-center text-sm text-muted">
                    未找到符合当前搜索或筛选条件的题目。
                  </div>
                ) : (
                  <div className="grid gap-3.5">
                    {paginatedQuestions.map((q) => (
                      <BankQuestionCardItem
                        key={`${q.tid}-${q.questionId}`}
                        question={q}
                        courseTitle={courseTitles[q.cid]}
                      />
                    ))}

                    {/* 分页控制器 */}
                    {totalPages > 1 && (
                      <Pagination className="justify-center pt-2">
                        <Pagination.Summary>
                          显示第 {startItem}–{endItem} 条，共 {totalItems} 条题目
                        </Pagination.Summary>
                        <Pagination.Content>
                          <Pagination.Item>
                            <Pagination.Previous
                              isDisabled={safePage <= 1}
                              onPress={() => setBankPage((p) => Math.max(1, p - 1))}
                            >
                              <Pagination.PreviousIcon />
                              <span>上一页</span>
                            </Pagination.Previous>
                          </Pagination.Item>
                          {Array.from({ length: totalPages }, (_, i) => i + 1)
                            .filter(
                              (p) => p === 1 || p === totalPages || Math.abs(p - safePage) <= 2,
                            )
                            .map((p) => (
                              <Pagination.Item key={p}>
                                <Pagination.Link
                                  isActive={p === safePage}
                                  onPress={() => setBankPage(p)}
                                >
                                  {p}
                                </Pagination.Link>
                              </Pagination.Item>
                            ))}
                          <Pagination.Item>
                            <Pagination.Next
                              isDisabled={safePage >= totalPages}
                              onPress={() => setBankPage((p) => Math.min(totalPages, p + 1))}
                            >
                              <span>下一页</span>
                              <Pagination.NextIcon />
                            </Pagination.Next>
                          </Pagination.Item>
                        </Pagination.Content>
                      </Pagination>
                    )}
                  </div>
                )}
              </Modal.Body>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>

      {/* 题目搜索卡片 */}
      <Card id="uooc-qa-vector-search" aria-labelledby="uooc-qa-search-title">
        <Card.Header>
          <Card.Title id="uooc-qa-search-title">题目搜索</Card.Title>
          <Card.Description>
            结合特征向量投影与服务端共享题库，支持按题干语义或关键词高速检索。
          </Card.Description>
        </Card.Header>
        <Card.Content className="grid gap-3">
          <div className="flex flex-col gap-2 sm:flex-row">
            <div className="flex-1">
              <TextField
                aria-label="题目搜索"
                value={searchQuery}
                isDisabled={searching}
                onChange={setSearchQuery}
              >
                <Input
                  placeholder="输入题干文本或关键词检索题目..."
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void handleSearch();
                    }
                  }}
                />
                <Description>
                  支持 512 维语义特征向量与关键词匹配，结合向量缓存与服务端高速检索
                </Description>
              </TextField>
            </div>
            <Button
              className="self-start sm:self-auto"
              variant="secondary"
              isDisabled={searching || !searchQuery.trim()}
              onPress={() => void handleSearch()}
            >
              {searching ? <Spinner size="sm" /> : <LuSearch aria-hidden="true" />}
              题目搜索
            </Button>
          </div>

          {searchResults.length > 0 && (
            <div className="mt-3 flex items-center justify-between rounded-xl border border-separator/50 bg-surface/40 p-3.5 text-xs">
              <span className="text-muted">
                已检索到 <strong className="text-foreground">{searchResults.length}</strong>{" "}
                道相关题目
              </span>
              <Button size="sm" variant="secondary" onPress={() => setSearchModalOpen(true)}>
                查看搜索结果
              </Button>
            </div>
          )}
        </Card.Content>
      </Card>

      {/* 题目搜索结果弹窗 Modal */}
      <Modal isOpen={searchModalOpen} onOpenChange={setSearchModalOpen}>
        <Modal.Backdrop>
          <Modal.Container size="cover" className="max-w-4xl">
            <Modal.Dialog aria-label="题目搜索结果">
              <Modal.CloseTrigger aria-label="关闭" />
              <Modal.Header className="flex flex-wrap items-center justify-between gap-3 pr-10">
                <div className="grid gap-1">
                  <Modal.Heading>题目搜索结果</Modal.Heading>
                  <p className="text-xs text-muted">
                    搜索词：「{searchQuery}」· 共匹配到 {searchResults.length}{" "}
                    条相关题目（按语义相似度降序排列）
                  </p>
                </div>
                <Switch
                  size="sm"
                  isSelected={showHtmlTags}
                  onChange={setShowHtmlTags}
                  aria-label="显示 HTML 标签"
                >
                  <Switch.Content className="text-xs">
                    <Switch.Control>
                      <Switch.Thumb />
                    </Switch.Control>
                    HTML 标签
                  </Switch.Content>
                </Switch>
              </Modal.Header>
              <Modal.Body className="max-h-[75vh] overflow-y-auto grid gap-4 p-4 sm:p-6">
                {convertedSearchResults.length === 0 ? (
                  <div className="rounded-xl border border-dashed border-separator/60 p-8 text-center text-sm text-muted">
                    未找到与「{searchQuery}」匹配的题目。
                  </div>
                ) : (
                  <div className="grid gap-3.5">
                    {convertedSearchResults.map(({ question, similarity, source }) => (
                      <BankQuestionCardItem
                        key={`${question.tid}-${question.questionId}`}
                        question={question}
                        similarity={similarity}
                        sourceOverride={source}
                        courseTitle={question.cid ? courseTitles[question.cid] : undefined}
                      />
                    ))}
                  </div>
                )}
              </Modal.Body>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>

      <AlertDialog isOpen={confirmClearBank} onOpenChange={setConfirmClearBank}>
        <AlertDialog.Backdrop>
          <AlertDialog.Container size="sm">
            <AlertDialog.Dialog>
              <AlertDialog.Icon status="danger" />
              <AlertDialog.Header>
                <AlertDialog.Heading>确认清空题库缓存？</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body>
                确定要清空已收录的试题缓存文件（qa-bank.json）吗？清空后如需恢复，可随时点击【重新构建题库】或通过测验流程自动重新发现与收录。
              </AlertDialog.Body>
              <AlertDialog.Footer>
                <AlertDialog.CloseTrigger className="dialog-cancel" />
                <Button
                  variant="danger"
                  isDisabled={clearingBank}
                  onPress={async () => {
                    setConfirmClearBank(false);
                    setClearingBank(true);
                    try {
                      const count = await invoke<number>("clear_uooc_local_bank");
                      toast.success(`已清空题库缓存（共清除 ${count} 道题目）`);
                      await refreshLocalQuestions();
                    } catch (e: unknown) {
                      const msg =
                        typeof e === "string"
                          ? e
                          : (e as { message?: string })?.message || "清空题库缓存失败";
                      toast.danger("清空题库缓存失败", { description: msg });
                    } finally {
                      setClearingBank(false);
                    }
                  }}
                >
                  确认清空
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      </AlertDialog>

      <BankRebuildModal
        isOpen={rebuildModalOpen}
        onOpenChange={setRebuildModalOpen}
        isBuilding={rebuildingBank}
        onComplete={() => void refreshLocalQuestions()}
      />
    </div>
  );
}
