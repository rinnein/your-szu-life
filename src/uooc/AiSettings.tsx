import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Checkbox, LoadingIndicator, Select, SelectOption, Button } from "material/react";
import { Banner, NumberField, bindField, toast } from "../ui";
import type { AiConfig, QuizMode } from "./types";

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
  const [config, setConfig] = useState<AiConfig>();
  const [models, setModels] = useState<{ main: string[]; jev: string[] }>({ main: [], jev: [] });
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");
  useEffect(() => {
    let live = true;
    void invoke<AiConfig>("get_uooc_ai_config")
      .then((v) => {
        if (live && v?.main) {
          setConfig(v);
          onMode(v.defaultMode);
        }
      })
      .catch((cause) => {
        if (live)
          setLoadError(
            typeof cause === "string"
              ? cause
              : cause instanceof Error
                ? cause.message
                : "无法读取 AI 配置",
          );
      });
    return () => {
      live = false;
    };
  }, []);
  const save = async () => {
    if (!config) return;
    const preferences = {
      mainModel: config.main.model,
      jevModel: config.jev.model,
      jevEnabled: config.jevEnabled,
      confidence: config.confidence,
      defaultMode: config.defaultMode,
      batchSize: config.batchSize ?? 5,
      reasoningEffort: config.reasoningEffort ?? "default",
      includeReasoning: config.includeReasoning ?? true,
      webSearch: config.webSearch ?? true,
    };
    const next = await invoke<AiConfig>("save_uooc_ai_preferences", { preferences });
    setConfig(next);
    onMode(next.defaultMode);
    return next;
  };
  const handleSave = async () => {
    setBusy(true);
    try {
      await save();
      toast.success("配置已保存");
    } catch (e) {
      const msg =
        typeof e === "string" ? e : e instanceof Error ? e.message : "保存配置失败，请检查配置";
      toast.danger("保存配置失败", { description: msg });
    } finally {
      setBusy(false);
    }
  };
  const handleLoadModels = async (provider: "main" | "jev") => {
    setBusy(true);
    try {
      const found = await invoke<string[]>("list_uooc_ai_models", { provider });
      setModels((old) => ({ ...old, [provider]: found }));
      toast.success(`已加载 ${found.length} 个模型，选择后保存配置`);
    } catch (e) {
      const msg = typeof e === "string" ? e : e instanceof Error ? e.message : "加载模型列表失败";
      toast.danger("加载模型列表失败", { description: msg });
    } finally {
      setBusy(false);
    }
  };
  if (!config)
    return (
      <Card variant="outlined">
        <div slot="headline">测验 AI</div>
        <div className="p-4">
          {loadError ? (
            <Banner status="danger" role="alert">
              <Banner.Title>AI 配置出错</Banner.Title>
              <Banner.Description>{loadError}</Banner.Description>
            </Banner>
          ) : (
            <p role="status">正在读取 AI 配置…</p>
          )}
        </div>
      </Card>
    );
  return (
    <Card variant="outlined">
      <div slot="headline" className="flex flex-wrap items-center justify-between gap-2 w-full">
        <span className="font-semibold">测验 AI</span>
        <Button variant="outlined" onClick={onOpenAiSettings}>
          前往 AI 连接设置
        </Button>
      </div>
      <div slot="subhead">API 地址和 Key 在全局设置中管理。测验内容会发送给你配置的供应商。</div>
      <div className="grid gap-4 p-4">
        {(["main", "jev"] as const).map((provider) => (
          <div className="grid gap-3" key={provider}>
            <h3 className="font-semibold">
              {provider === "main" ? "主模型 · OpenAI 兼容接口" : "Jev · TypeSafe System One"}
            </h3>
            {/*
              输入框与「加载列表」按钮同一行；空间不足时按钮换到下一行，
              而不是被压窄或溢出。`min-w-0` 让输入框可以真正收缩。
            */}
            <div className="flex flex-wrap items-end gap-3">
              <div
                className="min-w-0 flex-1 basis-64"
                {...bindField((model) =>
                  setConfig((old) =>
                    old ? { ...old, [provider]: { ...old[provider], model } } : old,
                  ),
                )}
                onClick={() => {
                  if (
                    models[provider].length === 0 &&
                    !busy &&
                    config[provider].baseUrl &&
                    config[provider].hasKey
                  ) {
                    void handleLoadModels(provider);
                  }
                }}
                onFocus={() => {
                  if (
                    models[provider].length === 0 &&
                    !busy &&
                    config[provider].baseUrl &&
                    config[provider].hasKey
                  ) {
                    void handleLoadModels(provider);
                  }
                }}
              >
                <Select
                  label={`${provider === "main" ? "主模型" : "Jev"}名称`}
                  value={config[provider].model}
                  disabled={busy || !config[provider].baseUrl || !config[provider].hasKey}
                  onChange={(event: any) => {
                    const model = event.detail?.value ?? event.target?.value ?? "";
                    setConfig({ ...config, [provider]: { ...config[provider], model } });
                  }}
                >
                  {models[provider].map((model) => (
                    <SelectOption key={model} value={model}>
                      {model}
                    </SelectOption>
                  ))}
                  {config[provider].model && !models[provider].includes(config[provider].model) && (
                    <SelectOption value={config[provider].model}>
                      {config[provider].model}
                    </SelectOption>
                  )}
                </Select>
              </div>
              <Button
                variant="tonal"
                disabled={busy}
                onClick={() => void handleLoadModels(provider)}
                className="shrink-0"
              >
                加载{provider === "main" ? "主模型" : "Jev"}列表
              </Button>
            </div>
          </div>
        ))}
        <Checkbox
          checked={config.jevEnabled}
          disabled={busy}
          onChange={(e: any) =>
            setConfig({ ...config, jevEnabled: Boolean(e.detail?.checked ?? e.target?.checked) })
          }
        >
          启用 Jev 选择题辅助复核
        </Checkbox>
        <NumberField
          minValue={0.5}
          maxValue={1}
          step={0.05}
          value={config.confidence}
          onChange={(confidence) => setConfig({ ...config, confidence })}
          disabled={busy}
          label="Jev 判断阈值"
          description="阈值用于转交人工，不代表答案正确率；图片题不经过 Jev 复核。"
        />
        <NumberField
          minValue={1}
          maxValue={20}
          step={1}
          value={config.batchSize ?? 10}
          onChange={(batchSize) => setConfig({ ...config, batchSize })}
          disabled={busy}
          label="题目合并批次大小"
          description="每次向 AI 发起请求时合并处理的最大题目数量（1-20，默认 10）。合并题目可减少多轮会话反复新开并注入上下文的开销。"
        />
        <ReasoningEffortPicker
          value={config.reasoningEffort ?? "default"}
          onChange={(reasoningEffort) => setConfig({ ...config, reasoningEffort })}
          disabled={busy}
        />
        <Checkbox
          checked={config.includeReasoning ?? true}
          disabled={busy}
          onChange={(e: any) =>
            setConfig({
              ...config,
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
          checked={config.webSearch ?? true}
          disabled={busy}
          onChange={(e: any) =>
            setConfig({
              ...config,
              webSearch: Boolean(e.detail?.checked ?? e.target?.checked),
            })
          }
        >
          要求 AI 使用联网搜索工具
        </Checkbox>
        <p className="text-xs text-muted -mt-2">
          在遇到有疑问的题目或冷门知识点时，允许 AI 自动发起 web_search
          工具调用检索互联网权威资料后再作答。
        </p>
        <QuizModePicker
          value={config.defaultMode}
          disabled={busy}
          onChange={(defaultMode) => setConfig({ ...config, defaultMode })}
        />
        <p className="field-description">
          默认手动答题：需自行作答或在弹窗中选择 AI
          答题。请求批准：先生成整卷答案，确认后才提交。YOLO 未通过时停止当前课程，其他课程继续。
        </p>
        <Button disabled={busy} onClick={() => void handleSave()}>
          保存运行设置
        </Button>
        {busy && <LoadingIndicator size={24} aria-label="正在处理 AI 配置" />}
      </div>
    </Card>
  );
}
