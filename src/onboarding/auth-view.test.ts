/** @vitest-environment jsdom */

import { describe, expect, it } from "vite-plus/test";
import { authStepView, isTerminalAuthFailure } from "./auth-view";
import type { AuthState } from "../auth";

const authenticated: AuthState = { Authenticated: { studentId: "1" } } as AuthState;
const authenticating: AuthState = { Authenticating: null } as AuthState;
const awaitingProfile: AuthState = {
  AwaitingProfile: { studentId: "2024", name: "张三" },
} as AuthState;
const unauthenticated: AuthState = { Unauthenticated: null } as AuthState;

describe("authStepView", () => {
  it("maps each auth state to exactly one view", () => {
    expect(authStepView(authenticated, { autoAttempt: "idle" })).toEqual({ kind: "authenticated" });
    expect(authStepView(authenticating, { autoAttempt: "idle" })).toEqual({
      kind: "authenticating",
    });
    expect(authStepView(awaitingProfile, { autoAttempt: "idle" })).toEqual({
      kind: "awaitingProfile",
      studentId: "2024",
      name: "张三",
    });
  });

  it("shows the restoring view only while a reauth attempt may still succeed", () => {
    expect(authStepView(unauthenticated, { reauth: true, autoAttempt: "idle" })).toEqual({
      kind: "restoring",
    });
    expect(authStepView(unauthenticated, { reauth: true, autoAttempt: "running" })).toEqual({
      kind: "restoring",
    });
    // 重认证失败或用户选择手动后落到凭据表单。
    expect(authStepView(unauthenticated, { reauth: true, autoAttempt: "failed" })).toEqual({
      kind: "credentials",
    });
  });

  it("never hides a real in-flight state behind the restoring view", () => {
    // 自动尝试进行中但后端已经在认证：仍然展示认证进度，不能回到「正在恢复」。
    expect(authStepView(authenticating, { reauth: true, autoAttempt: "running" })).toEqual({
      kind: "authenticating",
    });
  });

  it("treats only unrecoverable states as terminal", () => {
    expect(isTerminalAuthFailure(authenticated)).toBe(false);
    expect(isTerminalAuthFailure(unauthenticated)).toBe(false);
    expect(isTerminalAuthFailure({ AccountMismatch: null } as AuthState)).toBe(true);
  });
});
