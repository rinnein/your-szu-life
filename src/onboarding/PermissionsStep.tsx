import { PermissionSettings } from "../PermissionSettings";
import type { StepProps } from "./step-types";

export function PermissionsStep({ onComplete, registerNext }: StepProps) {
  return <PermissionSettings onboarding onComplete={onComplete} registerNext={registerNext} />;
}
