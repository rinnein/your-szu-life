import { Card } from "material/react";
import { Banner, toast } from "../ui";
import { AiConnectionForm } from "../AiConnectionSettings";
import type { StepProps } from "./step-types";

export function AiStep({ onComplete }: StepProps) {
  return (
    <div className="grid gap-4">
      <Banner status="info">
        <Banner.Title>未配置 AI 时无法使用 AI 功能</Banner.Title>
        <Banner.Description>
          例如 UOOC 的 AI 答题与解析。Jev 是可选的选择题复核模型，不配置也不影响基本 AI
          功能；这里只需要填写主模型的地址和 Key。
        </Banner.Description>
      </Banner>
      <Card variant="outlined">
        <div slot="headline">AI 配置</div>
        <div slot="subhead">只填写主模型即可；Jev 可以在设置 → AI 配置中按需开启。</div>
        <div className="p-4">
          <AiConnectionForm
            showJev={false}
            submitLabel="保存并继续"
            onSaved={async (connections) => {
              if (!connections.main.baseUrl || !connections.main.hasKey) {
                toast.info("主模型尚未配置完整；可以继续填写，或跳过此步骤");
                return;
              }
              await onComplete();
            }}
          />
        </div>
      </Card>
    </div>
  );
}
