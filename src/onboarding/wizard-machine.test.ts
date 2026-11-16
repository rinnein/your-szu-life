/** @vitest-environment jsdom */

import { describe, expect, it } from "vite-plus/test";
import { buildPlan, createWizardState, wizardReducer, type WizardState } from "./wizard-machine";
import type { OnboardingState, OnboardingStep, OnboardingStepStatus } from "./api";

function step(
  id: string,
  status: OnboardingStepStatus = "pending",
  skippable = true,
): OnboardingStep {
  return { id, status, skippable, revision: 0 };
}

function state(...steps: OnboardingStep[]): OnboardingState {
  return {
    steps,
    needsSetup: steps.some((item) => item.status === "pending"),
    currentStep: steps.find((item) => item.status === "pending")?.id ?? null,
  };
}

const ids = (plan: WizardState["plan"]) => plan.map((item) => item.id);

describe("buildPlan", () => {
  it("keeps only pending steps and prepends reauth when signed out", () => {
    const plan = buildPlan(
      state(step("welcome", "configured"), step("auth", "configured"), step("storage")),
      false,
    );
    expect(ids(plan)).toEqual(["auth", "storage"]);
    expect(plan[0].reauth).toBe(true);
  });

  it("does not duplicate the auth step when it is already pending and signed out", () => {
    const plan = buildPlan(state(step("auth"), step("storage")), false);
    expect(ids(plan)).toEqual(["auth", "storage"]);
  });
});

describe("wizardReducer", () => {
  const initial = () =>
    createWizardState(
      state(step("auth"), step("sync"), step("storage"), step("dormitory"), step("appearance")),
      true,
    );

  it("skips steps the server completed in the same round and lands on the next one", () => {
    // 同步步骤完成后，后端把 dormitory 也标成 configured：它必须从计划里消失。
    const atSync = wizardReducer(initial(), {
      type: "serverState",
      state: state(
        step("auth", "configured"),
        step("sync"),
        step("storage"),
        step("dormitory"),
        step("appearance"),
      ),
    });
    expect(ids(atSync.plan)).toEqual(["auth", "sync", "storage", "dormitory", "appearance"]);
    expect(ids(atSync.plan)[atSync.index]).toBe("sync");

    const afterSync = wizardReducer(atSync, {
      type: "serverState",
      state: state(
        step("auth", "configured"),
        step("sync", "configured"),
        step("storage"),
        step("dormitory", "configured"),
        step("appearance"),
      ),
    });
    expect(ids(afterSync.plan)).toEqual(["auth", "sync", "storage", "appearance"]);
    expect(ids(afterSync.plan)[afterSync.index]).toBe("storage");
  });

  it("finishes exactly once when the last step completes", () => {
    const only = createWizardState(state(step("storage")), true);
    const done = wizardReducer(only, {
      type: "serverState",
      state: state(step("storage", "configured")),
    });
    expect(done.finished).toBe(true);
    // finished 后索引停在最后一步，重复事件不会把 finished 复位。
    expect(
      wizardReducer(done, { type: "serverState", state: state(step("storage", "configured")) })
        .finished,
    ).toBe(true);
  });

  it("inserts a reauth step at the front when the session is lost mid-wizard", () => {
    // 已登录且认证步骤已完成：计划里没有 auth，失效时插一个只用于重认证的步骤。
    const signedIn = createWizardState(
      state(
        step("auth", "configured"),
        step("sync"),
        step("storage"),
        step("dormitory"),
        step("appearance"),
      ),
      true,
    );
    const lost = wizardReducer(signedIn, { type: "sessionLost" });
    expect(ids(lost.plan)).toEqual(["auth", "sync", "storage", "dormitory", "appearance"]);
    expect(lost.plan[0].reauth).toBe(true);
    expect(lost.index).toBe(0);
  });

  it("rewinds to the existing auth step instead of inserting a second one", () => {
    let current = initial();
    current = wizardReducer(current, {
      type: "serverState",
      state: state(
        step("auth", "configured"),
        step("sync"),
        step("storage"),
        step("dormitory"),
        step("appearance"),
      ),
    });
    // 现在停在 sync（index 1），认证步骤还在计划里（位置 0）。
    expect(ids(current.plan)[current.index]).toBe("sync");
    const lost = wizardReducer(current, { type: "sessionLost" });
    expect(ids(lost.plan)).toEqual(["auth", "sync", "storage", "dormitory", "appearance"]);
    expect(lost.index).toBe(0);
  });

  it("advances automatically once a reauth step is authenticated", () => {
    const signedIn = createWizardState(state(step("auth", "configured"), step("storage")), true);
    const lost = wizardReducer(signedIn, { type: "sessionLost" });
    expect(lost.plan[0].reauth).toBe(true);
    const restored = wizardReducer(lost, { type: "sessionRestored" });
    expect(restored.index).toBe(1);
    // 普通步骤不会因为会话恢复而前进。
    expect(wizardReducer(restored, { type: "sessionRestored" }).index).toBe(1);
  });

  it("goes back but never before the first step", () => {
    const advanced = wizardReducer(initial(), {
      type: "serverState",
      state: state(
        step("auth", "configured"),
        step("sync"),
        step("storage"),
        step("dormitory"),
        step("appearance"),
      ),
    });
    expect(advanced.index).toBe(1);
    expect(wizardReducer(advanced, { type: "back" }).index).toBe(0);
    expect(wizardReducer(initial(), { type: "back" }).index).toBe(0);
  });
});
