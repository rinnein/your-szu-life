import McpSettings from "../McpSettings";
import type { StepProps } from "./step-types";

export function McpStep({ onComplete, registerNext }: StepProps) {
  return <McpSettings onboarding onComplete={onComplete} registerNext={registerNext} />;
}
