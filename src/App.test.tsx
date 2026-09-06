import { describe, expect, it } from "vitest";
import { authFailureFor, normalizeAuthState } from "./App";

describe("authentication recovery UI", () => {
  it("keeps dedicated recovery copy for credential capture failures", () => {
    expect(authFailureFor({ CredentialCaptureFailed: null })).toMatchObject({
      title: "本次登录未能保存凭据",
      action: "重新登录",
    });
  });

  it("distinguishes session persistence and account-mismatch failures", () => {
    expect(authFailureFor({ SessionCaptureFailed: null })?.title).toContain("安全保存会话");
    expect(authFailureFor({ AccountMismatch: null })?.title).toContain("账号不一致");
  });

  it("keeps student-id setup limited to a campus-card supplement", () => {
    expect("补充校园卡号").toContain("校园卡号");
  });
});

describe("auth state transport normalization", () => {
  it("accepts serde's camelCase enum payloads", () => {
    expect(normalizeAuthState("authenticating")).toEqual({ Authenticating: null });
    expect(normalizeAuthState("credentialCaptureFailed")).toEqual({
      CredentialCaptureFailed: null,
    });
    expect(normalizeAuthState("sessionCaptureFailed")).toEqual({
      SessionCaptureFailed: null,
    });
    expect(normalizeAuthState("credentialStorageFailed")).toEqual({
      CredentialStorageFailed: null,
    });
    expect(normalizeAuthState("accountMismatch")).toEqual({ AccountMismatch: null });
    expect(normalizeAuthState({ awaitingProfile: { studentId: "1234567890" } })).toEqual({
      AwaitingProfile: { name: null, studentId: "1234567890" },
    });
  });

  it("recovers student ID from the pre-fix snake_case event payload", () => {
    expect(normalizeAuthState({ awaitingProfile: { student_id: "1234567890" } })).toEqual({
      AwaitingProfile: { name: null, studentId: "1234567890" },
    });
    expect(normalizeAuthState({ Authenticated: { student_id: "1234567890" } })).toEqual({
      Authenticated: { name: null, studentId: "1234567890" },
    });
  });
});
