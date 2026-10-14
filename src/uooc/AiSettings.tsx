import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Button,
  Card,
  Checkbox,
  ComboBox,
  Description,
  Alert,
  Input,
  Label,
  ListBox,
  NumberField,
  Select,
  Spinner,
} from "@heroui/react";
import type { AiConfig, QuizMode } from "./types";

export const modeLabels: Record<QuizMode, string> = {
  supervised: "手动答题",
  approval: "请求批准",
  yolo: "YOLO",
};
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
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
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
          setError(
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
    };
    const next = await invoke<AiConfig>("save_uooc_ai_preferences", { preferences });
    setConfig(next);
    onMode(next.defaultMode);
    return next;
  };
  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setMessage("");
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : "操作失败，请检查配置");
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
          {error ? (
            <Alert status="danger" role="alert">
              <Alert.Content>
                <Alert.Title>AI 配置出错</Alert.Title>
                <Alert.Description>{error}</Alert.Description>
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
        {error && (
          <Alert status="danger" role="alert">
            <Alert.Content>
              <Alert.Title>AI 配置出错</Alert.Title>
              <Alert.Description>{error}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {message && (
          <Alert status="success" role="status">
            <Alert.Content>
              <Alert.Title>{message}</Alert.Title>
            </Alert.Content>
          </Alert>
        )}
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
              onPress={() =>
                void run(async () => {
                  const found = await invoke<string[]>("list_uooc_ai_models", { provider });
                  setModels((old) => ({ ...old, [provider]: found }));
                  setMessage(`已加载 ${found.length} 个模型，选择后保存配置`);
                })
              }
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
        <QuizModePicker
          value={config.defaultMode}
          disabled={busy}
          onChange={(defaultMode) => setConfig({ ...config, defaultMode })}
        />
        <Description>
          默认手动答题：需自行作答或在弹窗中选择 AI
          答题。请求批准：先生成整卷答案，确认后才提交。YOLO 未通过时停止当前课程，其他课程继续。
        </Description>
        <Button
          isDisabled={busy}
          onPress={() =>
            void run(async () => {
              await save();
              setMessage("配置已保存");
            })
          }
        >
          保存运行设置
        </Button>
        {busy && <Spinner aria-label="正在处理 AI 配置" />}
      </Card.Content>
    </Card>
  );
}
