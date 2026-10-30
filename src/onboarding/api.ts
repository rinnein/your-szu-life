import { invoke } from "@tauri-apps/api/core";

export type OnboardingStepStatus = "pending" | "configured" | "skipped";

export type OnboardingStep = {
  id: string;
  status: OnboardingStepStatus;
  skippable: boolean;
  revision: number;
};

export type OnboardingState = {
  steps: OnboardingStep[];
  needsSetup: boolean;
  currentStep: string | null;
};

export type WallpaperImage = {
  dataUrl: string;
  width: number;
  height: number;
};

export async function getOnboardingState(): Promise<OnboardingState> {
  return invoke<OnboardingState>("get_onboarding_state");
}

export async function completeOnboardingStep(id: string): Promise<OnboardingState> {
  return invoke<OnboardingState>("complete_onboarding_step", { id });
}

export async function skipOnboardingStep(id: string): Promise<OnboardingState> {
  return invoke<OnboardingState>("skip_onboarding_step", { id });
}

export async function beginSetupAuthentication(
  campusCardNumber: string,
  password: string,
): Promise<void> {
  await invoke("begin_setup_authentication", {
    campusCardNumber,
    password,
    userAgent: navigator.userAgent,
  });
}

export async function cancelAuthentication(): Promise<void> {
  await invoke("cancel_authentication");
}

export async function consumeAuthFailureNotice(): Promise<string | null> {
  return invoke<string | null>("consume_auth_failure_notice");
}

export async function getWallpaperImage(): Promise<WallpaperImage> {
  return invoke<WallpaperImage>("get_wallpaper_image");
}
