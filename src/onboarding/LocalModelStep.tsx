import { Button, Card } from "material/react";
import { LocalModelCard } from "../LocalModelSettings";
import type { StepProps } from "./step-types";

export function LocalModelStep({ onComplete }: StepProps) {
  return (
    <div className="grid gap-4">
      <Card variant="outlined">
        <div slot="headline">本地离线模型</div>
        <div slot="subhead">
          OCR 用于选课系统与 OJ 验证码识别，Embedding 用于 UOOC 题库检索。模型只保存在本机；
          暂时不需要可以跳过，之后在设置 → 本地模型中下载。
        </div>
      </Card>
      <LocalModelCard kind="ocr" />
      <LocalModelCard kind="embedding" />
      <Button variant="filled" onClick={() => void onComplete()}>
        下一步
      </Button>
    </div>
  );
}
