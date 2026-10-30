import AiConnectionSettings from "../AiConnectionSettings";
import type { StepProps } from "./step-types";

export function AiStep({ onComplete, registerNext }: StepProps) {
  return <AiConnectionSettings onboarding onComplete={onComplete} registerNext={registerNext} />;
}
