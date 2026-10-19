import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Alert,
  Button,
  Card,
  Checkbox,
  ComboBox,
  Description,
  Input,
  Label,
  ListBox,
  NumberField,
  Select,
  Spinner,
  toast,
} from "@heroui/react";
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
    <Select
      aria-label="AI 推理强度"
      value={value || "default"}
      onChange={(v) => {
        if (typeof v === "string") onChange(v);
      }}
      isDisabled={disabled}
    >
      <Label>AI 推理强度 (Reasoning Effort)</Label>
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Description>
        针对支持思考链的模型（如 o1、o3、DeepSeek R1 等兼容推理模型）调整推理预算与思考深度。
      </Description>
      <Select.Popover>
        <ListBox>
          {Object.entries(reasoningEffortLabels).map(([key, label]) => (
            <ListBox.Item key={key} id={key} textValue={label}>
              {label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
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
    <Select
      aria-label="答题模式"
      value={value}
      onChange={(v) => {
        if (v === "approval" || v === "supervised" || v === "yolo") onChange(v);
      }}
      isDisabled={disabled}
    >
      <Label>答题模式</Label>
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      {description && <Description>{description}</Description>}
      <Select.Popover>
        <ListBox>
          {Object.entries(modeLabels).map(([key, label]) => (
            <ListBox.Item key={key} id={key} textValue={label}>
              {label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
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
      <Card>
        <Card.Header>
          <Card.Title>测验 AI</Card.Title>
        </Card.Header>
        <Card.Content>
          {loadError ? (
            <Alert status="danger" role="alert">
              <Alert.Content>
                <Alert.Title>AI 配置出错</Alert.Title>
                <Alert.Description>{loadError}</Alert.Description>
              </Alert.Content>
            </Alert>
          ) : (
            <p role="status">正在读取 AI 配置…</p>
          )}
        </Card.Content>
      </Card>
    );
  return (
    <Card>
      <Card.Header>
        <Card.Title>测验 AI</Card.Title>
        <Card.Description>
          API 地址和 Key 在全局设置中管理。测验内容会发送给你配置的供应商。
        </Card.Description>
        <Button variant="outline" onPress={onOpenAiSettings}>
          前往 AI 连接设置
        </Button>
      </Card.Header>
      <Card.Content className="grid gap-4">
        {(["main", "jev"] as const).map((provider) => (
          <div className="grid gap-3" key={provider}>
            <h3 className="font-semibold">
              {provider === "main" ? "主模型 · OpenAI 兼容接口" : "Jev · TypeSafe System One"}
            </h3>
            <ComboBox
              allowsCustomValue
              inputValue={config[provider].model}
              onInputChange={(model) =>
                setConfig({ ...config, [provider]: { ...config[provider], model } })
              }
              isDisabled={busy || !config[provider].baseUrl || !config[provider].hasKey}
            >
              <Label>{provider === "main" ? "主模型" : "Jev"}名称</Label>
              <ComboBox.InputGroup>
                <Input placeholder="搜索或手动填写模型" />
                <ComboBox.Trigger />
              </ComboBox.InputGroup>
              <ComboBox.Popover>
                <ListBox>
                  {models[provider].map((model) => (
                    <ListBox.Item id={model} key={model} textValue={model}>
                      {model}
                      <ListBox.ItemIndicator />
                    </ListBox.Item>
                  ))}
                </ListBox>
              </ComboBox.Popover>
            </ComboBox>
            <Button
              variant="secondary"
              isDisabled={busy}
              onPress={() => void handleLoadModels(provider)}
            >
              加载{provider === "main" ? "主模型" : "Jev"}列表
            </Button>
          </div>
        ))}
        <Checkbox
          isSelected={config.jevEnabled}
          onChange={(jevEnabled) => setConfig({ ...config, jevEnabled })}
          isDisabled={busy}
        >
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          <Checkbox.Content>启用 Jev 选择题辅助复核</Checkbox.Content>
        </Checkbox>
        <NumberField
          minValue={0.5}
          maxValue={1}
          step={0.05}
          value={config.confidence}
          onChange={(confidence) => setConfig({ ...config, confidence })}
          isDisabled={busy}
        >
          <Label>Jev 判断阈值</Label>
          <NumberField.Group>
            <NumberField.DecrementButton />
            <NumberField.Input />
            <NumberField.IncrementButton />
          </NumberField.Group>
          <Description>阈值用于转交人工，不代表答案正确率；图片题不经过 Jev 复核。</Description>
        </NumberField>
        <NumberField
          minValue={1}
          maxValue={20}
          step={1}
          value={config.batchSize ?? 10}
          onChange={(batchSize) => setConfig({ ...config, batchSize })}
          isDisabled={busy}
        >
          <Label>题目合并批次大小</Label>
          <NumberField.Group>
            <NumberField.DecrementButton />
            <NumberField.Input />
            <NumberField.IncrementButton />
          </NumberField.Group>
          <Description>
            每次向 AI 发起请求时合并处理的最大题目数量（1-20，默认
            10）。合并题目可减少多轮会话反复新开并注入上下文的开销。
          </Description>
        </NumberField>
        <ReasoningEffortPicker
          value={config.reasoningEffort ?? "default"}
          onChange={(reasoningEffort) => setConfig({ ...config, reasoningEffort })}
          disabled={busy}
        />
        <Checkbox
          isSelected={config.includeReasoning ?? true}
          onChange={(includeReasoning) => setConfig({ ...config, includeReasoning })}
          isDisabled={busy}
        >
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          <Checkbox.Content>在提示词中要求 AI 返回推理过程与解题理由</Checkbox.Content>
        </Checkbox>
        <Description className="text-xs text-muted -mt-2">
          启用后，AI 判题将在每道题目下方展示得出该结论的理由/解析，便于人工复核与审查。
        </Description>
        <Checkbox
          isSelected={config.webSearch ?? true}
          onChange={(webSearch) => setConfig({ ...config, webSearch })}
          isDisabled={busy}
        >
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          <Checkbox.Content>要求 AI 使用联网搜索工具</Checkbox.Content>
        </Checkbox>
        <Description className="text-xs text-muted -mt-2">
          在遇到有疑问的题目或冷门知识点时，允许 AI 自动发起 web_search
          工具调用检索互联网权威资料后再作答。
        </Description>
        <QuizModePicker
          value={config.defaultMode}
          disabled={busy}
          onChange={(defaultMode) => setConfig({ ...config, defaultMode })}
        />
        <Description>
          默认手动答题：需自行作答或在弹窗中选择 AI
          答题。请求批准：先生成整卷答案，确认后才提交。YOLO 未通过时停止当前课程，其他课程继续。
        </Description>
        <Button isDisabled={busy} onPress={() => void handleSave()}>
          保存运行设置
        </Button>
        {busy && <Spinner aria-label="正在处理 AI 配置" />}
      </Card.Content>
    </Card>
  );
}
