import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Checkbox, LoadingIndicator, Select, SelectOption, Button } from "material/react";
import { Banner, NumberField, SettingList, SettingRow, toast } from "../../ui";
import { errorMessage, useAsyncLoad } from "../../async-state";
import { usePending } from "../../use-pending";
import type { QuizMode, UoocAiConfig } from "../state/types";
import { aiVendorNeedsKey } from "../../ai/types";

export const modeLabels: Record<QuizMode, string> = {
  supervised: "手动答题",
  approval: "请求批准",
  yolo: "YOLO",
};

export const reasoningEffortLabels: Record<string, string> = {
  default: "默认 (由模型自行决定)",
  low: "低推理强度 (Low)",
  medium: "中等推理强度 (Medium)",
  high: "高推理强度 (High)",
};

export function ReasoningEffortPicker({
  value,
  onChange,
  disabled = false,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex flex-col gap-1">
      <Select
        aria-label="AI 推理强度"
        label="AI 推理强度 (Reasoning Effort)"
        value={value || "default"}
        disabled={disabled}
        onChange={(e) => {
          const v = (e.target as any).value;
          if (typeof v === "string") onChange(v);
        }}
      >
        {Object.entries(reasoningEffortLabels).map(([key, label]) => (
          <SelectOption key={key} value={key}>
            {label}
          </SelectOption>
        ))}
      </Select>
      <p className="text-xs text-muted">
        针对支持思考链的模型（如 o1、o3、DeepSeek R1 等兼容推理模型）调整推理预算与思考深度。
      </p>
    </div>
  );
}

export function QuizModePicker({
  value,
  onChange,
  disabled = false,
  description,
}: {
  value: QuizMode;
  onChange: (value: QuizMode) => void;
  disabled?: boolean;
  description?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <Select
        aria-label="答题模式"
        label="答题模式"
        value={value}
        disabled={disabled}
        onChange={(e) => {
          const v = (e.target as any).value;
          if (v === "approval" || v === "supervised" || v === "yolo") onChange(v);
        }}
      >
        {Object.entries(modeLabels).map(([key, label]) => (
          <SelectOption key={key} value={key}>
            {label}
          </SelectOption>
        ))}
      </Select>
      {description && <p className="text-xs text-muted">{description}</p>}
    </div>
  );
}

export function AiSettings({
  onMode,
  onOpenAiSettings,
}: {
  onMode: (mode: QuizMode) => void;
  onOpenAiSettings: () => void;
}) {
  const { state, reload } = useAsyncLoad<UoocAiConfig>(() => invoke("get_uooc_ai_config"));
  const [draft, setDraft] = useState<UoocAiConfig>();
  const pending = usePending();
  const saving = pending.isPending("save");

  useEffect(() => {
    if (state.status !== "ready" || draft) return;
    setDraft(state.data);
    onMode(state.data.defaultMode);
  }, [state, draft, onMode]);

  const save = async () => {
    if (!draft) return;
    const outcome = await pending.run("save", () =>
      invoke<UoocAiConfig>("save_uooc_ai_preferences", {
        preferences: {
          batchSize: draft.batchSize,
          reasoningEffort: draft.reasoningEffort,
          includeReasoning: draft.includeReasoning,
          webSearch: draft.webSearch,
          defaultMode: draft.defaultMode,
        },
      }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("保存测验 AI 设置失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    setDraft(outcome.value);
    onMode(outcome.value.defaultMode);
    toast.success("测验 AI 设置已保存");
  };

  if (!draft) {
    return (
      <Card variant="outlined">
        <div slot="headline">测验 AI</div>
        <div className="grid gap-3">
          {state.status === "error" ? (
            <>
              <Banner status="danger" role="alert">
                <Banner.Title>无法读取测验 AI 设置</Banner.Title>
                <Banner.Description>{state.message}</Banner.Description>
              </Banner>
              <div>
                <Button variant="outlined" onClick={() => void reload()}>
                  重新读取
                </Button>
              </div>
            </>
          ) : (
            <p className="status-line" role="status">
              <LoadingIndicator size={16} aria-hidden="true" />
              正在读取测验 AI 设置…
            </p>
          )}
        </div>
      </Card>
    );
  }

  const provider = draft.provider;

  return (
    <Card variant="outlined">
      <div slot="headline">测验 AI</div>
      <div slot="subhead">
        模型由「设置 → AI 配置」统一管理；这里只调整答题批次、推理与联网等运行参数。
      </div>
      <div className="grid gap-4">
        {!provider && (
          <Banner status="warning">
            <Banner.Title>尚未配置默认模型</Banner.Title>
            <Banner.Description>
              题库匹配后仍有未确定题目时，AI 答题会提示缺少默认模型；手动答题不受影响。
            </Banner.Description>
          </Banner>
        )}
        <SettingList>
          <SettingRow
            title="默认模型"
            description={
              provider
                ? `${provider.name} · ${provider.modelName}${
                    aiVendorNeedsKey(provider.vendor) && !provider.hasKey ? " · 缺少 API Key" : ""
                  }`
                : "尚未选择默认模型"
            }
          >
            <Button variant="tonal" onClick={onOpenAiSettings}>
              前往 AI 配置
            </Button>
          </SettingRow>
        </SettingList>

        <NumberField
          minValue={1}
          maxValue={20}
          step={1}
          value={draft.batchSize}
          onChange={(batchSize) => setDraft({ ...draft, batchSize })}
          disabled={saving}
          label="题目合并批次大小"
          description="每次通过 ACP 子部件处理的题目数量（1-20，默认 5）。合并题目可减少多轮会话反复新开并注入上下文的开销。"
        />
        <ReasoningEffortPicker
          value={draft.reasoningEffort}
          onChange={(reasoningEffort) => setDraft({ ...draft, reasoningEffort })}
          disabled={saving}
        />
        <Checkbox
          checked={draft.includeReasoning}
          disabled={saving}
          onChange={(e: any) =>
            setDraft({
              ...draft,
              includeReasoning: Boolean(e.detail?.checked ?? e.target?.checked),
            })
          }
        >
          在提示词中要求 AI 返回推理过程与解题理由
        </Checkbox>
        <p className="text-xs text-muted -mt-2">
          启用后，AI 判题将在每道题目下方展示得出该结论的理由/解析，便于人工复核与审查。
        </p>
        <Checkbox
          checked={draft.webSearch}
          disabled={saving}
          onChange={(e: any) =>
            setDraft({ ...draft, webSearch: Boolean(e.detail?.checked ?? e.target?.checked) })
          }
        >
          允许 ACP 答题子部件使用联网搜索工具
        </Checkbox>
        <p className="text-xs text-muted -mt-2">
          在遇到有疑问的题目或冷门知识点时，允许 AI 自动发起 web_search
          检索互联网权威资料后再作答；模型不支持工具调用时会自动降级。
        </p>
        <QuizModePicker
          value={draft.defaultMode}
          disabled={saving}
          onChange={(defaultMode) => setDraft({ ...draft, defaultMode })}
        />
        <p className="field-description">
          默认手动答题：需自行作答或在弹窗中选择 AI
          答题。请求批准：先生成整卷答案，确认后才提交。YOLO 未通过时停止当前课程，其他课程继续。
        </p>
        <div>
          <Button disabled={saving} onClick={() => void save()}>
            {saving && <LoadingIndicator size={16} />}
            {saving ? "保存中…" : "保存运行设置"}
          </Button>
        </div>
      </div>
    </Card>
  );
}
