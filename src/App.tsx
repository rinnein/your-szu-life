import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import appIcon from "../src-tauri/icons/icon.png";
import "./App.css";
type Profile = {
  studentId: string;
  campusCardNumber: string;
  hasPassword: boolean;
  name?: string | null;
};
type UserData = { studentId: string; name?: string | null };
type AwaitingProfileState = { name?: string | null; studentId?: string | null };
type AuthenticatedState = { name?: string | null; studentId: string };
type AuthState =
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

// Serde serializes Rust unit enum variants as strings (for example
// `"authenticating"`), while variants carrying data are objects. Normalize
// both forms before the UI performs object-key checks.
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
        // Accept the old snake_case payload too, so an already-open setup
        // page can recover the verified ID after this update.
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
const features = ["课表", "选课", "培养方案检查", "成绩查询", "校园网登录", "网费与电费"];

type AuthFailure = { title: string; detail: string; action: string };

// Failure rendering is entirely state-driven. In particular, no refresh path
// begins a new authentication attempt on its own, so a late RPC can never
// replace a completed setup state with the initial screen.
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
function ProfileForm({
  initial,
  name,
  onlyCampusCard = false,
  onSaved,
}: {
  initial?: Partial<Profile>;
  name?: string | null;
  onlyCampusCard?: boolean;
  onSaved: () => void;
}) {
  const [studentId, setStudentId] = useState(initial?.studentId ?? "");
  const [campusCardNumber, setCampusCardNumber] = useState(initial?.campusCardNumber ?? "");
  const [displayName, setDisplayName] = useState(name ?? initial?.name ?? "");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!/^\d{10}$/.test(studentId)) return setError("学号必须是 10 位数字");
    if (!/^\d{6}$/.test(campusCardNumber)) return setError("校园卡号必须是 6 位数字");
    if (!onlyCampusCard && !initial?.hasPassword && !password) return setError("密码不能为空");
    try {
      await invoke("confirm_profile", {
        input: {
          studentId,
          campusCardNumber,
          name: displayName || undefined,
          password: password || undefined,
        },
      });
      onSaved();
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <form className="profile-form" onSubmit={save}>
      <h2>{onlyCampusCard ? "补充校园卡号" : "确认账户资料"}</h2>
      <p className="muted">
        {onlyCampusCard
          ? "已验证的学号和密码已安全保存；请补充校园卡号以完成账户设置。"
          : "确认资料后即可进入工作台。"}
      </p>
      {!onlyCampusCard && (
        <label>
          姓名
          <input
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="登录后自动获取"
          />
        </label>
      )}
      <label>
        学号
        <input
          inputMode="numeric"
          value={studentId}
          disabled={onlyCampusCard}
          onChange={(e) => setStudentId(e.target.value)}
        />
      </label>
      <label>
        校园卡号
        <input
          inputMode="numeric"
          value={campusCardNumber}
          onChange={(e) => setCampusCardNumber(e.target.value)}
          autoFocus={onlyCampusCard}
        />
      </label>
      {!onlyCampusCard && (
        <label>
          密码
          <input
            type="password"
            placeholder={initial?.hasPassword ? "已保存（输入新密码可覆盖）" : "请输入密码"}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
      )}
      {error && <p className="error">{error}</p>}
      <button className="primary" type="submit">
        保存并进入工作台
      </button>
    </form>
  );
}
function App() {
  const [state, setState] = useState<AuthState>({ Checking: null });
  const [profile, setProfile] = useState<Profile>();
  const [userData, setUserData] = useState<UserData>();
  const [view, setView] = useState<"home" | "settings">("home");
  const [menu, setMenu] = useState(false);
  const [placeholder, setPlaceholder] = useState<string>();
  const refresh = async () => {
    try {
      const [rawState, nextProfile, nextUserData] = await Promise.all([
        invoke<unknown>("get_auth_state"),
        invoke<Profile | null>("get_account_profile"),
        invoke<UserData | null>("get_user_data"),
      ]);
      const s = normalizeAuthState(rawState);
      setState(s);
      setProfile(nextProfile ?? undefined);
      setUserData(nextUserData ?? undefined);
    } catch (error) {
      // A refresh started before navigating to the official page can finish
      // after the auth-state event. Never let that late failure overwrite a
      // terminal state and send the user back to the initial setup screen.
      console.error("读取统一认证状态失败", error);
      setState((current) => ("Checking" in current ? { Unauthenticated: null } : current));
    }
  };
  useEffect(() => {
    refresh();
    let unlisten: (() => void) | undefined;
    listen<unknown>("auth-state-changed", (event) => {
      const next = normalizeAuthState(event.payload);
      setState(next);
      invoke<UserData | null>("get_user_data").then((data) => setUserData(data ?? undefined));
      if ("Authenticated" in next) {
        invoke<Profile | null>("get_account_profile").then((p) => setProfile(p ?? undefined));
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);
  if ("Checking" in state)
    return (
      <main className="shell centered">
        <div className="loader">正在检查统一认证状态…</div>
      </main>
    );
  if ("Unauthenticated" in state)
    return (
      <main className="shell centered">
        <div className="auth-card">
          <img className="brand-mark" src={appIcon} alt="泥深" />
          <h1>请完成统一认证</h1>
          <p className="muted">请在当前窗口完成深圳大学官方登录，登录成功后会自动返回工作台。</p>
          <button className="primary" onClick={() => invoke("begin_authentication")}>
            打开认证窗口
          </button>
        </div>
      </main>
    );
  if ("Authenticating" in state)
    return (
      <main className="shell centered">
        <div className="auth-card">
          <img className="brand-mark" src={appIcon} alt="泥深" />
          <h1>正在打开统一认证</h1>
          <p className="muted">
            请在当前窗口完成深圳大学官方登录。验证成功且会话安全保存后，应用会自动返回后续设置或工作台。
          </p>
        </div>
      </main>
    );
  const failure = authFailureFor(state);
  if (failure)
    return (
      <main className="shell centered">
        <div className="auth-card">
          <img className="brand-mark" src={appIcon} alt="泥深" />
          <h1>{failure.title}</h1>
          <p className="muted">{failure.detail}</p>
          <button className="primary" onClick={() => invoke("begin_authentication")}>
            {failure.action}
          </button>
        </div>
      </main>
    );
  if ("AwaitingProfile" in state) {
    // The state event is the primary source. The read-only user-data command
    // is a safe fallback for a stale event payload and never exposes a
    // password, cookie, or session token.
    const studentId = state.AwaitingProfile.studentId ?? userData?.studentId ?? "";
    const name = state.AwaitingProfile.name ?? userData?.name;
    return (
      <main className="shell centered">
        <ProfileForm
          key={studentId || "pending-profile"}
          onlyCampusCard
          name={name}
          initial={{ studentId, hasPassword: true }}
          onSaved={async () => {
            const saved = (await invoke<Profile>("get_account_profile")) ?? undefined;
            setProfile(saved);
            if (saved)
              setState({ Authenticated: { name: saved.name, studentId: saved.studentId } });
          }}
        />
      </main>
    );
  }
  if (view === "settings")
    return (
      <main className="shell centered">
        <ProfileForm
          initial={profile}
          name={profile?.name}
          onSaved={async () => {
            setProfile((await invoke<Profile>("get_account_profile")) ?? undefined);
            setView("home");
          }}
        />
        <button className="link back" onClick={() => setView("home")}>
          返回工作台
        </button>
      </main>
    );
  const auth =
    "Authenticated" in state ? state.Authenticated : { studentId: profile?.studentId ?? "" };
  return (
    <main className="shell">
      <header>
        <div>
          <div className="brand-line">
            <img className="brand-icon" src={appIcon} alt="" />
            <div className="eyebrow">YOUR SZU LIFE</div>
          </div>
          <h1>深大生活工作台</h1>
        </div>
        <div className="account-wrap">
          <button className="account" onClick={() => setMenu(!menu)}>
            <span className="avatar">{(auth.name ?? "同").slice(0, 1)}</span>
            {auth.studentId}
          </button>
          {menu && (
            <div className="menu">
              <div className="menu-name">{auth.name ?? "未设置姓名"}</div>
              <div className="menu-row">
                校园卡号 <strong>{profile?.campusCardNumber ?? "未设置"}</strong>
              </div>
              <button
                onClick={() => {
                  setMenu(false);
                  setView("settings");
                }}
              >
                设置
              </button>
              <button
                onClick={async () => {
                  setMenu(false);
                  await invoke("begin_authentication");
                }}
              >
                重新认证
              </button>
              <button
                className="danger"
                onClick={async () => {
                  if (confirm("确定注销并清除本机认证数据吗？")) {
                    await invoke("logout");
                    setMenu(false);
                    setProfile(undefined);
                    setState({ Unauthenticated: null });
                  }
                }}
              >
                注销
              </button>
            </div>
          )}
        </div>
      </header>
      <section className="hero">
        <div>
          <p className="eyebrow">WELCOME BACK</p>
          <h2>{auth.name ? `${auth.name}，欢迎回来` : "欢迎回来"}</h2>
          <p>从这里开始你的校园生活。</p>
        </div>
        <div className="hero-orb">深</div>
      </section>
      <h3>功能入口</h3>
      <div className="feature-grid">
        {features.map((f) => (
          <button className="feature" key={f} onClick={() => setPlaceholder(f)}>
            <span className="feature-icon">✦</span>
            <span>
              <strong>{f}</strong>
              <small>暂未实现 · 即将支持</small>
            </span>
            <span className="arrow">→</span>
          </button>
        ))}
      </div>
      {placeholder && (
        <div className="toast" onClick={() => setPlaceholder(undefined)}>
          {placeholder}功能即将支持，当前仅提供入口占位。<span>×</span>
        </div>
      )}
    </main>
  );
}
export default App;
