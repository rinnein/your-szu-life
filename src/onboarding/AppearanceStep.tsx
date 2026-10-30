import { AppearanceSettings } from "../AppearanceSettings";
import type { StepProps } from "./step-types";

export function AppearanceStep({ onComplete, registerNext }: StepProps) {
  return <AppearanceSettings onboarding onComplete={onComplete} registerNext={registerNext} />;
}
