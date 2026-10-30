import { LocalModelsSettings } from "../LocalModelSettings";
import type { StepProps } from "./step-types";

export function LocalModelStep({ onComplete, registerNext }: StepProps) {
  return <LocalModelsSettings onboarding onComplete={onComplete} registerNext={registerNext} />;
}
