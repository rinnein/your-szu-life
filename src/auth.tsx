import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Profile = {
  studentId: string;
  campusCardNumber: string;
  hasPassword: boolean;
  name?: string | null;
};
export type UserData = { studentId: string; name?: string | null };
export type AwaitingProfileState = { name?: string | null; studentId?: string | null };
export type AuthenticatedState = { name?: string | null; studentId: string };
export type AuthState =
  | { Checking: null }
  | { Unauthenticated: null }
  | { Authenticating: null }
  | { AwaitingProfile: AwaitingProfileState }
  | { Authenticated: AuthenticatedState }
  | { CredentialCaptureFailed: null }
  | { SessionCaptureFailed: null }
  | { CredentialStorageFailed: null }
  | { AccountMismatch: null }
  | { SessionExpired: null };

export function normalizeAuthState(raw: unknown): AuthState {
  if (typeof raw === "string") {
    const key = raw.charAt(0).toUpperCase() + raw.slice(1);
    if (
      key === "Checking" ||
      key === "Unauthenticated" ||
      key === "Authenticating" ||
      key === "CredentialCaptureFailed" ||
      key === "SessionCaptureFailed" ||
      key === "CredentialStorageFailed" ||
      key === "AccountMismatch" ||
      key === "SessionExpired"
    ) {
      return { [key]: null } as AuthState;
    }
  }
  if (raw && typeof raw === "object") {
    const value = raw as Record<string, unknown>;
    const normalizeProfileState = (payload: unknown) => {
      const profile = (payload && typeof payload === "object" ? payload : {}) as Record<
        string,
        unknown
      >;
      return {
        name: typeof profile.name === "string" ? profile.name : null,
        studentId:
          typeof profile.studentId === "string"
            ? profile.studentId
            : typeof profile.student_id === "string"
              ? profile.student_id
              : null,
      };
    };
    if ("awaitingProfile" in value)
      return { AwaitingProfile: normalizeProfileState(value.awaitingProfile) };
    if ("authenticated" in value) {
      const authenticated = normalizeProfileState(value.authenticated);
      if (!authenticated.studentId) throw new Error("认证状态中缺少学号");
      return { Authenticated: authenticated as AuthenticatedState };
    }
    if ("AwaitingProfile" in value)
      return { AwaitingProfile: normalizeProfileState(value.AwaitingProfile) };
    if ("Authenticated" in value) {
      const authenticated = normalizeProfileState(value.Authenticated);
      if (!authenticated.studentId) throw new Error("认证状态中缺少学号");
      return { Authenticated: authenticated as AuthenticatedState };
    }
  }
  throw new Error("无法识别统一认证状态");
}

export type AuthFailure = { title: string; detail: string; action: string };
export function authFailureFor(state: AuthState): AuthFailure | undefined {
  if ("SessionExpired" in state)
    return {
      title: "统一认证已过期",
      detail: "本机资料仍保留，请重新认证以恢复官方会话。",
      action: "重新认证",
    };
  if ("CredentialCaptureFailed" in state)
    return {
      title: "本次登录未能保存凭据",
      detail: "首次设置必须使用账号密码登录，应用未能在官方页面确认保存账号密码。",
      action: "重新登录",
    };
  if ("SessionCaptureFailed" in state)
    return {
      title: "本次登录未能安全保存会话",
      detail: "官方登录已完成，但会话未能写入系统凭据库；请重新认证。",
      action: "重新认证",
    };
  if ("CredentialStorageFailed" in state)
    return {
      title: "无法保存本机认证资料",
      detail: "系统凭据库写入失败，未保留本次登录资料；请检查系统凭据库后重试。",
      action: "重新登录",
    };
  if ("AccountMismatch" in state)
    return {
      title: "认证账号不一致",
      detail: "本次认证账号与本机保存的学号不同，现有账户资料未被覆盖。",
      action: "重新认证",
    };
}

type AuthContextValue = {
  state: AuthState;
  profile?: Profile;
  userData?: UserData;
  refresh: () => Promise<void>;
  setState: (state: AuthState) => void;
  setProfile: (profile?: Profile) => void;
};
const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>({ Checking: null });
  const [profile, setProfile] = useState<Profile>();
  const [userData, setUserData] = useState<UserData>();

  const refresh = async () => {
    try {
      const [rawState, nextProfile, nextUserData] = await Promise.all([
        invoke<unknown>("get_auth_state"),
        invoke<Profile | null>("get_account_profile"),
        invoke<UserData | null>("get_user_data"),
      ]);
      setState(normalizeAuthState(rawState));
      setProfile(nextProfile ?? undefined);
      setUserData(nextUserData ?? undefined);
    } catch (error) {
      console.error("读取统一认证状态失败", error);
      setState((current) => ("Checking" in current ? { Unauthenticated: null } : current));
    }
  };

  useEffect(() => {
    void refresh();
    for (const command of [
      "consume_dorm_login_failure_notice",
      "consume_staff_login_failure_notice",
    ]) {
      void invoke<{ message?: string } | null>(command)
        .then((notice) => {
          if (notice?.message && typeof window !== "undefined") {
            window.setTimeout(() => window.alert(`校园网自动登录失败：${notice.message}`), 0);
          }
        })
        .catch(() => undefined);
    }

    let unlisten: (() => void) | undefined;
    void listen<unknown>("auth-state-changed", (event) => {
      const next = normalizeAuthState(event.payload);
      setState(next);
      void invoke<UserData | null>("get_user_data").then((data) => setUserData(data ?? undefined));
      if ("Authenticated" in next) {
        void invoke<Profile | null>("get_account_profile").then((p) => setProfile(p ?? undefined));
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const value = useMemo(
    () => ({ state, profile, userData, refresh, setState, setProfile }),
    [state, profile, userData],
  );
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const value = useContext(AuthContext);
  if (!value) throw new Error("useAuth must be used inside AuthProvider");
  return value;
}
