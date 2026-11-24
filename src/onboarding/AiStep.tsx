import { Card } from "material/react";
import { Banner, toast } from "../ui";
import { ProviderForm } from "../ai/ProviderForm";
import type { StepProps } from "./step-types";

export function AiStep({ onComplete, registerNext }: StepProps) {
  return (
    <>
      <Banner status="info">
        <Banner.Title>配置一个 AI 提供商</Banner.Title>
        <Banner.Description>
          已内置 UOOC 智能小优（Dify
          公共聊天）作为默认提供商，无需密钥即可使用；也可以在这里换成自己的 AI
          提供商。未配置也可以稍后在设置 → AI 配置中完成，手动答题不受影响。
        </Banner.Description>
      </Banner>
      <Card variant="outlined">
        <div slot="headline">AI 配置</div>
        <div slot="subhead">
          支持 OpenAI 兼容、Anthropic、Gemini、Ollama 与 Dify 公共聊天。API Key
          保存在系统凭据存储中，不会回显。
        </div>
        <ProviderForm
          onboarding
          submitLabel="保存并继续"
          registerNext={registerNext}
          onSaved={async (config) => {
            if (!config.default) {
              toast.info("尚未选择默认模型，可以稍后在设置 → AI 配置中完成");
              return;
            }
            await onComplete?.();
          }}
        />
      </Card>
    </>
  );
}
