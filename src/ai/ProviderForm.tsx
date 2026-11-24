import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Button,
  Checkbox,
  LoadingIndicator,
  Select,
  SelectOption,
  TextField,
} from "material/react";
import { Banner, NumberField, bindField, toast } from "../ui";
import { errorMessage } from "../async-state";
import { usePending } from "../use-pending";
import { useRegisterNextAction, type StepNextAction } from "../step-next-action";
import {
  AI_ADDABLE_VENDORS,
  AI_PRESETS,
  AI_VENDOR_LABELS,
  aiVendorBaseUrlPlaceholder,
  aiVendorNeedsKey,
  type AiConfigView,
  type AiDetectionView,
  type AiModelConfig,
  type AiProviderView,
  type AiVendor,
} from "./types";

export function ProviderForm({
  provider,
  defaultModel,
  submitLabel = "保存提供商",
  onboarding = false,
  registerNext,
  onSaved,
}: {
  provider?: AiProviderView | null;
  defaultModel?: string;
  submitLabel?: string;
  onboarding?: boolean;
  registerNext?: (action: StepNextAction | null) => void;
  onSaved: (config: AiConfigView) => void | Promise<void>;
}) {
  const [presetId, setPresetId] = useState("custom");
  const [name, setName] = useState(provider?.name ?? "");
  const [vendor, setVendor] = useState<AiVendor>(provider?.vendor ?? "customendpoint");
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? "");
  const [apiKey, setApiKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [models, setModels] = useState<AiModelConfig[]>(provider?.models ?? []);
  const [modelId, setModelId] = useState(defaultModel ?? provider?.models?.[0]?.id ?? "");
  const [detection, setDetection] = useState<AiDetectionView | null>(null);
  const [detectError, setDetectError] = useState("");
  const pending = usePending();
  const saving = pending.isPending("save");
  const detecting = pending.isPending("detect");
  const matching = pending.isPending("match");
  const busy = saving || detecting || matching;

  const currentModel = models.find((model) => model.id === modelId);

  const applyPreset = (id: string) => {
    setPresetId(id);
    const preset = AI_PRESETS.find((item) => item.id === id);
    if (!preset || preset.id === "custom") return;
    setVendor(preset.vendor);
    setBaseUrl(preset.baseUrl);
    setName(preset.name);
    setModels([]);
    setModelId("");
    setDetection(null);
    setDetectError("");
  };

  const replaceModel = (matched: AiModelConfig) => {
    setModels((current) => {
      const index = current.findIndex((model) => model.id === matched.id);
      if (index < 0) {
        return [...current, matched].sort((a, b) => a.id.localeCompare(b.id));
      }
      return current.map((model, position) =>
        position === index ? { ...matched, name: model.name || matched.name } : model,
      );
    });
  };

  /** 按 vendor + 模型名重新匹配能力；只改 id 匹配的条目。 */
  const matchModel = async (
    rawId: string,
    targetVendor: AiVendor = vendor,
    targetBaseUrl: string = baseUrl,
  ) => {
    const id = rawId.trim();
    if (!id || !targetBaseUrl.trim()) return;
    const outcome = await pending.run("match", () =>
      invoke<AiModelConfig>("preview_ai_model", {
        input: { vendor: targetVendor, baseUrl: targetBaseUrl, model: id },
      }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("自动匹配模型能力失败", {
          description: errorMessage(outcome.error),
        });
      }
      return;
    }
    replaceModel(outcome.value);
  };

  const updateCurrentModel = (patch: Partial<AiModelConfig>) => {
    setModels((current) =>
      current.map((model) => (model.id === modelId ? { ...model, ...patch } : model)),
    );
  };

  const detect = async () => {
    setDetectError("");
    const outcome = await pending.run("detect", () =>
      invoke<AiDetectionView>("detect_ai_provider", {
        input: {
          baseUrl,
          apiKey: apiKey || null,
          providerId: clearKey ? null : (provider?.id ?? null),
          protocol: null,
        },
      }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) {
        const message = errorMessage(outcome.error);
        setDetectError(message);
        toast.danger("自动探测失败", { description: message });
      }
      return;
    }
    const result = outcome.value;
    if (result.vendor === "dify") {
      toast.info("Dify 公共聊天已内置，无需添加", {
        description: "请在「模型提供商」列表里直接使用内置的 UOOC 智能小优。",
      });
      return;
    }
    setDetection(result);
    setVendor(result.vendor);
    setBaseUrl(result.baseUrl);
    setModels(result.models);
    const current = result.models.find((model) => model.id === modelId);
    setModelId(current?.id ?? result.models[0]?.id ?? "");
    toast.success(
      `已识别为 ${AI_VENDOR_LABELS[result.vendor]}${
        result.models.length > 0 ? `，${result.models.length} 个模型` : ""
      }`,
    );
  };

  const save = async () => {
    if (!baseUrl.trim()) {
      toast.danger("请填写 API 地址");
      return;
    }
    const id = modelId.trim();
    let nextModels = models;
    if (id && !models.some((model) => model.id === id)) {
      const outcome = await pending.run("save", () =>
        invoke<AiModelConfig>("preview_ai_model", {
          input: { vendor, baseUrl, model: id },
        }),
      );
      if (!outcome.ok) {
        if (!outcome.skipped) {
          toast.danger("无法匹配模型能力", { description: errorMessage(outcome.error) });
        }
        return;
      }
      nextModels = [...models, outcome.value];
      setModels(nextModels);
    }
    const outcome = await pending.run("save", () =>
      invoke<AiConfigView>("upsert_ai_provider", {
        input: {
          id: provider?.id,
          name,
          vendor,
          baseUrl,
          apiKey: apiKey || null,
          clearKey,
          models: nextModels,
          model: id || null,
        },
      }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("保存 AI 提供商失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    toast.success("AI 提供商已保存");
    await onSaved(outcome.value);
  };

  useRegisterNextAction(
    registerNext,
    onboarding ? { label: submitLabel, disabled: busy, busy: saving, run: save } : null,
  );

  return (
    <form className="settings-form" onSubmit={(event) => event.preventDefault()}>
      {!provider && (
        <Select
          aria-label="AI 提供商预设"
          label="预设"
          value={presetId}
          disabled={busy}
          onChange={(event: any) =>
            applyPreset(event.target?.value ?? event.detail?.value ?? "custom")
          }
        >
          {AI_PRESETS.map((preset) => (
            <SelectOption key={preset.id} value={preset.id}>
              {preset.label}
            </SelectOption>
          ))}
        </Select>
      )}

      <TextField
        label="提供商名称"
        value={name}
        placeholder="例如 OpenAI、DeepSeek、本地 Ollama"
        disabled={busy}
        {...bindField(setName)}
      />
      <Select
        aria-label="模型供应商"
        label="供应商"
        value={vendor}
        disabled={busy}
        onChange={(event: any) => {
          const value = event.target?.value ?? event.detail?.value;
          if (!AI_ADDABLE_VENDORS.includes(value)) return;
          setVendor(value);
          setDetection(null);
          if (modelId.trim()) void matchModel(modelId, value);
        }}
      >
        {AI_ADDABLE_VENDORS.map((item) => (
          <SelectOption key={item} value={item}>
            {AI_VENDOR_LABELS[item]}
          </SelectOption>
        ))}
      </Select>
      <TextField
        label="API 地址"
        value={baseUrl}
        placeholder={aiVendorBaseUrlPlaceholder(vendor)}
        disabled={busy}
        {...bindField(setBaseUrl)}
      />
      <p className="field-description">
        可填写完整端点（如 /chat/completions、/v1/messages），保存时会自动归一化。
      </p>
      {aiVendorNeedsKey(vendor) && (
        <>
          <TextField
            type="password"
            label="API Key"
            value={apiKey}
            placeholder={provider?.hasKey ? "已保存；留空保持不变" : "填写密钥"}
            autocomplete="new-password"
            disabled={busy || clearKey}
            {...bindField((value) => {
              setApiKey(value);
              if (value) setClearKey(false);
            })}
          />
          {provider?.hasKey && (
            <Checkbox
              checked={clearKey}
              disabled={busy || apiKey !== ""}
              onChange={(event: any) => {
                const checked = Boolean(event.detail?.checked ?? event.target?.checked);
                setClearKey(checked);
                if (checked) setApiKey("");
              }}
            >
              删除已保存的密钥
            </Checkbox>
          )}
        </>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="tonal" disabled={busy || !baseUrl.trim()} onClick={() => void detect()}>
          {detecting && <LoadingIndicator size={16} />}
          {detecting ? "正在探测…" : "自动探测供应商与模型"}
        </Button>
        {detection && (
          <span className="text-xs text-muted">
            {AI_VENDOR_LABELS[detection.vendor]}
            {detection.verified ? " · 凭据已验证" : ""}
          </span>
        )}
      </div>
      {detectError && (
        <Banner status="warning">
          <Banner.Title>自动探测失败</Banner.Title>
          <Banner.Description>{detectError} 你仍然可以手动选择供应商并保存。</Banner.Description>
        </Banner>
      )}

      {models.length > 0 && (
        <Select
          aria-label="已配置模型"
          label="已配置模型"
          value={models.some((model) => model.id === modelId) ? modelId : ""}
          disabled={busy}
          onChange={(event: any) => {
            const value = event.target?.value ?? event.detail?.value;
            if (typeof value === "string" && value) setModelId(value);
          }}
        >
          {models.map((model) => (
            <SelectOption key={model.id} value={model.id}>
              {model.name || model.id}
            </SelectOption>
          ))}
        </Select>
      )}
      <TextField
        label="默认模型"
        value={modelId}
        placeholder="例如 gpt-4.1、claude-sonnet、qwen3"
        disabled={busy}
        {...bindField(setModelId)}
        onBlur={() => {
          if (!models.some((model) => model.id === modelId.trim())) {
            void matchModel(modelId);
          }
        }}
      />
      <p className="field-description">
        从已配置模型中选择，或直接填写模型 ID；填写后会按供应商与模型名自动匹配能力。
      </p>

      {currentModel && (
        <div className="card-group">
          <div className="card-group-title">模型能力</div>
          <Checkbox
            checked={currentModel.toolCalling}
            disabled={busy}
            onChange={(event: any) => {
              const checked = Boolean(event.detail?.checked ?? event.target?.checked);
              updateCurrentModel({ toolCalling: checked });
            }}
          >
            支持工具调用（toolCalling）
          </Checkbox>
          <Checkbox
            checked={currentModel.vision}
            disabled={busy}
            onChange={(event: any) => {
              const checked = Boolean(event.detail?.checked ?? event.target?.checked);
              updateCurrentModel({ vision: checked });
            }}
          >
            支持图片输入（vision）
          </Checkbox>
          <NumberField
            label="上下文（最大输入 tokens）"
            minValue={1}
            maxValue={100_000_000}
            step={1024}
            value={currentModel.maxInputTokens ?? 0}
            disabled={busy}
            onChange={(value) => updateCurrentModel({ maxInputTokens: value > 0 ? value : null })}
          />
          <NumberField
            label="最大输出 tokens"
            minValue={1}
            maxValue={10_000_000}
            step={256}
            value={currentModel.maxOutputTokens ?? 0}
            disabled={busy}
            onChange={(value) => updateCurrentModel({ maxOutputTokens: value > 0 ? value : null })}
          />
          <div>
            <Button
              variant="text"
              disabled={busy || !baseUrl.trim() || !modelId.trim()}
              onClick={() => void matchModel(modelId)}
            >
              按供应商与模型名重新匹配能力
            </Button>
          </div>
        </div>
      )}

      {!onboarding && (
        <div>
          <Button variant="filled" disabled={saving || !baseUrl.trim()} onClick={() => void save()}>
            {saving && <LoadingIndicator size={16} />}
            {saving ? "保存中…" : submitLabel}
          </Button>
        </div>
      )}
    </form>
  );
}
