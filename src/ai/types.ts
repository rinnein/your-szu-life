export type AiProtocol = "openAiCompatible" | "anthropic" | "gemini" | "ollama" | "difyChat";

export const AI_PROTOCOL_LABELS: Record<AiProtocol, string> = {
  openAiCompatible: "OpenAI 兼容",
  anthropic: "Anthropic",
  gemini: "Google Gemini",
  ollama: "Ollama",
  difyChat: "Dify 公共聊天",
};

/** 与 VS Code chatLanguageModels.json 的 vendor 对齐；dify 是内置扩展。 */
export type AiVendor =
  | "customendpoint"
  | "openai"
  | "azure"
  | "anthropic"
  | "gemini"
  | "deepseek"
  | "openrouter"
  | "ollama"
  | "dify";

export const AI_VENDOR_LABELS: Record<AiVendor, string> = {
  customendpoint: "自定义 OpenAI 兼容",
  openai: "OpenAI",
  azure: "Azure OpenAI",
  anthropic: "Anthropic",
  gemini: "Google Gemini",
  deepseek: "DeepSeek",
  openrouter: "OpenRouter",
  ollama: "Ollama",
  dify: "Dify 公共聊天",
};

/** 新增提供商时可选的 vendor；Dify 使用内置且不可新增。 */
export const AI_ADDABLE_VENDORS: AiVendor[] = [
  "openai",
  "azure",
  "anthropic",
  "gemini",
  "deepseek",
  "openrouter",
  "ollama",
  "customendpoint",
];

export function aiVendorNeedsKey(vendor: AiVendor): boolean {
  return vendor !== "ollama" && vendor !== "dify";
}

export function aiVendorBaseUrlPlaceholder(vendor: AiVendor): string {
  switch (vendor) {
    case "openai":
      return "https://api.openai.com/v1";
    case "azure":
      return "https://<resource>.openai.azure.com/openai/deployments/<deployment>";
    case "deepseek":
      return "https://api.deepseek.com/v1";
    case "openrouter":
      return "https://openrouter.ai/api/v1";
    case "anthropic":
      return "https://api.anthropic.com";
    case "gemini":
      return "https://generativelanguage.googleapis.com";
    case "ollama":
      return "http://127.0.0.1:11434";
    default:
      return "https://example.com/v1";
  }
}

/** VS Code schema 的 model 条目。 */
export type AiModelConfig = {
  id: string;
  name: string;
  url: string;
  toolCalling: boolean;
  vision: boolean;
  maxInputTokens?: number | null;
  maxOutputTokens?: number | null;
  contextWindow?: number | null;
};

export type AiProviderView = {
  id: string;
  vendor: AiVendor;
  name: string;
  protocol: AiProtocol;
  baseUrl: string;
  models: AiModelConfig[];
  hasKey: boolean;
  builtin: boolean;
  appCode?: string;
};

export type AiSelectedModel = {
  providerId: string;
  model: string;
};

export type AiConfigView = {
  providers: AiProviderView[];
  default: AiSelectedModel | null;
};

export type AiDetectionView = {
  protocol: AiProtocol;
  vendor: AiVendor;
  baseUrl: string;
  models: AiModelConfig[];
  verified: boolean;
};

export type AiProviderInput = {
  id?: string;
  name: string;
  vendor: AiVendor;
  baseUrl: string;
  apiKey?: string | null;
  clearKey?: boolean;
  models?: AiModelConfig[];
  model?: string | null;
};

export type AiPreviewModelInput = {
  vendor: AiVendor;
  baseUrl: string;
  model: string;
};

export type AiPreset = {
  id: string;
  label: string;
  vendor: AiVendor;
  baseUrl: string;
  name: string;
  needsKey: boolean;
};

export const AI_PRESETS: AiPreset[] = [
  {
    id: "openai",
    label: "OpenAI",
    vendor: "openai",
    baseUrl: "https://api.openai.com/v1",
    name: "OpenAI",
    needsKey: true,
  },
  {
    id: "azure",
    label: "Azure OpenAI",
    vendor: "azure",
    baseUrl: "",
    name: "Azure OpenAI",
    needsKey: true,
  },
  {
    id: "anthropic",
    label: "Anthropic",
    vendor: "anthropic",
    baseUrl: "https://api.anthropic.com",
    name: "Anthropic",
    needsKey: true,
  },
  {
    id: "gemini",
    label: "Google Gemini",
    vendor: "gemini",
    baseUrl: "https://generativelanguage.googleapis.com",
    name: "Google Gemini",
    needsKey: true,
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    vendor: "deepseek",
    baseUrl: "https://api.deepseek.com/v1",
    name: "DeepSeek",
    needsKey: true,
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    vendor: "openrouter",
    baseUrl: "https://openrouter.ai/api/v1",
    name: "OpenRouter",
    needsKey: true,
  },
  {
    id: "ollama",
    label: "Ollama",
    vendor: "ollama",
    baseUrl: "http://127.0.0.1:11434",
    name: "Ollama",
    needsKey: false,
  },
  {
    id: "custom",
    label: "自定义 / OpenAI 兼容",
    vendor: "customendpoint",
    baseUrl: "",
    name: "",
    needsKey: true,
  },
];
