import type { ComponentType } from "react";
import type { StepProps } from "./step-types";
import { AuthStep } from "./AuthStep";
import { DormStep } from "./DormStep";
import { AppearanceStep } from "./AppearanceStep";
import { LocalModelStep } from "./LocalModelStep";
import { AiStep } from "./AiStep";
import { McpStep } from "./McpStep";

export const STEP_META: Record<string, { label: string; component: ComponentType<StepProps> }> = {
  auth: { label: "账户与个人信息", component: AuthStep },
  dormitory: { label: "宿舍信息", component: DormStep },
  appearance: { label: "主题设置", component: AppearanceStep },
  local_model: { label: "本地模型", component: LocalModelStep },
  ai: { label: "AI 配置", component: AiStep },
  mcp: { label: "MCP 服务", component: McpStep },
};
