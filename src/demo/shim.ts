/**
 * `@tauri-apps/api/core` 的演示模式替身。
 *
 * Tauri 2 的 `__TAURI_INTERNALS__`（含 `invoke`）经 `Object.defineProperty` +
 * `Object.freeze` 安装，运行期不可改写；因此改在模块边界拦截：vite.config.ts
 * 把裸导入 `@tauri-apps/api/core` 解析到本文件，业务代码与官方插件拿到的
 * `invoke` 都经过这里的路由，而其余导出（`Channel`、`transformCallback` 等）
 * 从包内的真实文件原样转发——注意不要写裸导入，否则会再次命中 alias 形成自环。
 */
import { demoActive, setDemoActive } from "./demo-state";
import { mockAuthState, mockProfile, mockUserData } from "./fixtures/auth";
import {
  mockElectricityConstants,
  mockElectricityOptions,
  mockElectricityRecharges,
  mockElectricityResult,
  mockElectricitySettings,
} from "./fixtures/electricity";
import {
  mockDormSettings,
  mockDormStatus,
  mockNetworkOverview,
  mockStaffSettings,
  mockStaffStatus,
} from "./fixtures/campus-network";
import { mockUoocAssessments, mockUoocSnapshot } from "./fixtures/uooc";
import {
  mockOjContests,
  mockOjGroups,
  mockOjProgress,
  mockOjRecords,
  mockOjSnapshot,
} from "./fixtures/oj";
import { mockLmsSnapshot } from "./fixtures/lms";
import { mockJxxkSnapshot } from "./fixtures/patrol";
import { mockEnrollmentSnapshot } from "./fixtures/enrollment";
import { mockTodoItems } from "./fixtures/todos";

export * from "../../node_modules/@tauri-apps/api/core.js";

type InvokeArgs = Record<string, unknown> | number[] | ArrayBuffer | Uint8Array;
type ChannelLike = { send: (payload: unknown) => Promise<void> | void };

function channelFrom(args: InvokeArgs | undefined): ChannelLike | undefined {
  const channel = (args as { channel?: ChannelLike } | undefined)?.channel;
  return channel && typeof channel.send === "function" ? channel : undefined;
}

function unavailable(command: string): never {
  throw new Error(`演示模式下不可用：${command}`);
}

type MockHandler = (args?: InvokeArgs) => unknown;

/** 演示模式下按命令名分发的 mock；未列出的命令放行给真实后端。 */
const mockHandlers: Record<string, MockHandler> = {
  // 统一认证：状态用 mock，任何真实登录/登出入口一律拦截，保护真实凭据。
  get_auth_state: () => mockAuthState,
  get_account_profile: () => mockProfile,
  get_user_data: () => mockUserData,
  logout: () => undefined,
  cancel_authentication: () => undefined,
  begin_authentication: () => unavailable("begin_authentication"),
  begin_setup_authentication: () => unavailable("begin_setup_authentication"),
  refresh_auth_session: () => unavailable("refresh_auth_session"),
  consume_auth_failure_notice: () => null,

  // 电量。
  get_electricity_settings: () => mockElectricitySettings,
  save_electricity_settings: () => undefined,
  get_electricity_constants: () => mockElectricityConstants,
  get_electricity_options: () => mockElectricityOptions,
  get_cached_electricity: () => mockElectricityResult,
  query_electricity: () => mockElectricityResult,
  query_electricity_recharges: () => mockElectricityRecharges,

  // 校园网。
  get_network_overview: () => mockNetworkOverview,
  refresh_campus_network: () => mockNetworkOverview,
  get_dorm_login_status: () => mockDormStatus,
  get_dorm_network_settings: () => mockDormSettings,
  get_staff_network_settings: () => mockStaffSettings,
  get_staff_login_status: () => mockStaffStatus,
  consume_dorm_login_failure_notice: () => null,
  consume_staff_login_failure_notice: () => null,
  trigger_dorm_login: () => unavailable("trigger_dorm_login"),
  apply_staff_network_mode: () => unavailable("apply_staff_network_mode"),
  logout_campus_network: () => unavailable("logout_campus_network"),

  // 待办：经 Channel 推送，与后端的 initial/snapshot 事件形状一致。
  subscribe_todos: (args) => {
    const channel = channelFrom(args);
    if (channel) {
      void channel.send({ type: "initial", data: mockTodoItems });
      void channel.send({ type: "snapshot", data: mockTodoItems });
    }
    return null;
  },
  refresh_todos: () => undefined,

  // UOOC。
  get_uooc_snapshot: () => mockUoocSnapshot,
  refresh_uooc_progress: () => mockUoocSnapshot,
  subscribe_uooc_assessments: (args) => {
    const channel = channelFrom(args);
    if (channel) void channel.send({ type: "initial", data: mockUoocAssessments });
    return null;
  },
  refresh_uooc_assessments: () => undefined,
  open_uooc_quiz_portal: () => unavailable("open_uooc_quiz_portal"),

  // OJ。
  get_oj_snapshot: () => mockOjSnapshot,
  refresh_oj: () => mockOjSnapshot,
  get_oj_groups: () => mockOjGroups,
  get_oj_contests: () => mockOjContests,
  get_oj_records: () => mockOjRecords,
  get_oj_progress: () => mockOjProgress,
  save_oj_preferences: () => undefined,

  // 畅课。
  get_lms_snapshot: () => mockLmsSnapshot,
  refresh_lms: () => mockLmsSnapshot,

  // 巡课。
  get_jxxk_snapshot: () => mockJxxkSnapshot,
  refresh_jxxk: () => mockJxxkSnapshot,

  // 选课：单命令多 action，按 action 路由。
  enrollment_command: (args) => {
    const action = (args as { action?: unknown } | undefined)?.action;
    if (action === "snapshot" || action === "refresh") return mockEnrollmentSnapshot;
    if (typeof action === "object" && action !== null) {
      const type = (action as { type?: string }).type;
      if (type === "refresh" || type === "query") return mockEnrollmentSnapshot;
    }
    if (action === "probe") {
      return [{ mode: "campus", latency: 12, needs2fa: false, message: "" }];
    }
    if (action === "session") return { status: "ok", snapshot: mockEnrollmentSnapshot };
    return unavailable(`enrollment_command(${String(action)})`);
  },
  subscribe_enrollment_state: (args) => {
    const channel = channelFrom(args);
    if (channel) void channel.send({ type: "snapshot", data: mockEnrollmentSnapshot });
    return null;
  },
  open_academic_portal: () => unavailable("open_academic_portal"),
  open_enrollment_official: () => unavailable("open_enrollment_official"),
  open_network_fee_portal: () => unavailable("open_network_fee_portal"),
};

function passthroughInvoke<T>(
  cmd: string,
  args?: InvokeArgs,
  options?: Record<string, unknown>,
): Promise<T> {
  const internals = (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ as
    | { invoke?: (cmd: string, args?: InvokeArgs, options?: Record<string, unknown>) => Promise<T> }
    | undefined;
  if (!internals || typeof internals.invoke !== "function") {
    return Promise.reject(new Error(`Tauri IPC 不可用：${cmd}`));
  }
  return internals.invoke(cmd, args, options);
}

/**
 * 与真实 `invoke` 同签名的替身：非演示模式原样转发；演示模式下命中 mock 表的
 * 返回模拟数据，其余命令仍放行（真实后端在无会话时会优雅降级）。
 */
export function invoke<T>(
  cmd: string,
  args?: InvokeArgs,
  options?: Record<string, unknown>,
): Promise<T> {
  if (cmd === "set_demo_mode") {
    const enabled = Boolean((args as { enabled?: boolean } | undefined)?.enabled);
    // 先置位再透传：后端随命令发出的 auth-state-changed 可能先于 invoke 返回
    // 到达，紧随其后的 get_account_profile 等调用必须已经走 mock。该分支必须在
    // demoActive 检查之前——开启演示模式的那次调用本身还没处于演示模式。
    setDemoActive(enabled);
    return passthroughInvoke<T>(cmd, args, options).catch((cause) => {
      setDemoActive(!enabled);
      throw cause;
    });
  }
  if (!demoActive()) {
    return passthroughInvoke<T>(cmd, args, options);
  }
  const handler = mockHandlers[cmd];
  if (!handler) {
    return passthroughInvoke<T>(cmd, args, options);
  }
  try {
    return Promise.resolve(handler(args) as T);
  } catch (cause) {
    // handler 同步抛出（如拦截真实登录入口）时转成 rejected promise。
    return Promise.reject(cause);
  }
}
