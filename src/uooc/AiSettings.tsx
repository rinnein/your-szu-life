import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
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
  TextField,
} from "@heroui/react";
import type { AiConfig, QuizMode } from "./types";

export const modeLabels: Record<QuizMode, string> = {
  approval: "请求批准",
  supervised: "监督",
  yolo: "YOLO",
};
export function QuizModePicker({
  value,
  onChange,
  disabled = false,
}: {
  value: QuizMode;
  onChange: (value: QuizMode) => void;
  disabled?: boolean;
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
export function AiSettings({ onMode }: { onMode: (mode: QuizMode) => void }) {
  const [config, setConfig] = useState<AiConfig>();
  const [keys, setKeys] = useState({ main: "", jev: "" });
  const [clear, setClear] = useState({ main: false, jev: false });
  const [models, setModels] = useState<{ main: string[]; jev: string[] }>({ main: [], jev: [] });
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => {
    let live = true;
    void invoke<AiConfig>("get_uooc_ai_config")
      .then((v) => {
        if (live && v?.main) {
          setConfig(v);
          onMode(v.defaultMode);
        }
      })
      .catch(() => {
        if (live) setMessage("无法读取 AI 配置");
      });
    return () => {
      live = false;
    };
  }, []);
  const save = async () => {
    if (!config) return;
    const input = {
      ...config,
      main: {
        baseUrl: config.main.baseUrl,
        model: config.main.model,
        apiKey: keys.main || null,
        clearKey: clear.main,
      },
      jev: {
        baseUrl: config.jev.baseUrl,
        model: config.jev.model,
        apiKey: keys.jev || null,
        clearKey: clear.jev,
      },
    };
    const next = await invoke<AiConfig>("save_uooc_ai_config", { config: input });
    setConfig(next);
    setKeys({ main: "", jev: "" });
    setClear({ main: false, jev: false });
    onMode(next.defaultMode);
    return next;
  };
  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setMessage("");
    try {
      await fn();
    } catch (e) {
      setMessage(typeof e === "string" ? e : "操作失败，请检查配置");
    } finally {
      setBusy(false);
    }
  };
  if (!config)
    return (
      <Card>
        <Card.Content>
          <p role="status">{message || "正在读取 AI 配置…"}</p>
        </Card.Content>
      </Card>
    );
  return (
    <Card>
      <Card.Header>
        <Card.Title>测验 AI</Card.Title>
        <Card.Description>
          API Key 保存于系统密钥存储。测验内容会发送给你配置的供应商。
        </Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-4">
        {(["main", "jev"] as const).map((provider) => (
          <div className="grid gap-3" key={provider}>
            <h3 className="font-semibold">
              {provider === "main" ? "主模型 · OpenAI 兼容接口" : "Jev · TypeSafe System One"}
            </h3>
            <TextField
              value={config[provider].baseUrl}
              onChange={(baseUrl) =>
                setConfig({ ...config, [provider]: { ...config[provider], baseUrl } })
              }
              isDisabled={busy}
            >
              <Label>{provider === "main" ? "主模型" : "Jev"} API 地址</Label>
              <Input
                placeholder={
                  provider === "main" ? "https://example.com/v1" : "https://api.typesafe.ai/v1"
                }
              />
            </TextField>
            <TextField
              type="password"
              value={keys[provider]}
              onChange={(value) => setKeys({ ...keys, [provider]: value })}
              isDisabled={busy}
            >
              <Label>{provider === "main" ? "主模型" : "Jev"} API Key</Label>
              <Input
                autoComplete="new-password"
                placeholder={config[provider].hasKey ? "已保存；留空保持不变" : "填写密钥"}
              />
            </TextField>
            <Checkbox
              isSelected={clear[provider]}
              onChange={(v) => setClear({ ...clear, [provider]: v })}
              isDisabled={busy}
            >
              <Checkbox.Control>
                <Checkbox.Indicator />
              </Checkbox.Control>
              <Checkbox.Content>
                删除已保存的{provider === "main" ? "主模型" : "Jev"}密钥
              </Checkbox.Content>
            </Checkbox>
            <ComboBox
              allowsCustomValue
              inputValue={config[provider].model}
              onInputChange={(model) =>
                setConfig({ ...config, [provider]: { ...config[provider], model } })
              }
              isDisabled={busy}
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
                  await save();
                  const found = await invoke<string[]>("list_uooc_ai_models", { provider });
                  setModels((old) => ({ ...old, [provider]: found }));
                  setMessage(`已加载 ${found.length} 个模型，选择后保存配置`);
                })
              }
            >
              保存并加载{provider === "main" ? "主模型" : "Jev"}列表
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
          默认请求批准：先生成整卷答案，确认后才提交。YOLO 未通过时停止当前课程，其他课程继续。
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
          保存 AI 配置
        </Button>
        {busy && <Spinner aria-label="正在处理 AI 配置" />}
        {message && <p role="status">{message}</p>}
      </Card.Content>
    </Card>
  );
}
