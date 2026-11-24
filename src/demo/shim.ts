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
import { mockUoocAiConfig, mockAiConfig } from "./fixtures/uooc";
import {
  mockOjContests,
  mockOjGroups,
  mockOjPaper,
  mockOjProgress,
  mockOjRecords,
  mockOjSnapshot,
} from "./fixtures/oj";
import { mockAppContentSnapshot, mockRemoteResourceBytes } from "./fixtures/app-content";
import { mockLmsSnapshot } from "./fixtures/lms";
import {
  mockSigninAutomation,
  mockSigninBuildings,
  mockSigninQwStatus,
  mockSigninRollcalls,
  mockSigninTimetable,
} from "./fixtures/signin";
import { mockJxxkSnapshot } from "./fixtures/patrol";
import { mockEnrollmentBatches, mockEnrollmentSnapshot } from "./fixtures/enrollment";
import { mockTodoItems } from "./fixtures/todos";
import { mockTimetableSnapshot } from "./fixtures/timetable";

export * from "../../node_modules/@tauri-apps/api/core.js";

type InvokeArgs = Record<string, unknown> | number[] | ArrayBuffer | Uint8Array;
/**
 * 前端 Channel 的接收端形态。
 *
 * `@tauri-apps/api` 的 `Channel` 只有 `onmessage`（写成 Rust 侧 send 时由
 * `__TAURI_INTERNALS__` 回调递送），**没有 `send`**。演示模式要把后端的
 * initial / snapshot 推给页面，只能直接调用 `onmessage`。
 */
type ChannelLike = { onmessage?: ((message: unknown) => void) | null };

function channelFrom(args: InvokeArgs | undefined): ChannelLike | undefined {
  const channel = (args as { channel?: ChannelLike } | undefined)?.channel;
  return channel && typeof channel.onmessage === "function" ? channel : undefined;
}

/** 按后端「Rust 侧 send」的等价语义投递一条消息。 */
function push(channel: ChannelLike, payload: unknown): void {
  channel.onmessage?.(payload);
}

function unavailable(command: string): never {
  throw new Error(`演示模式下不可用：${command}`);
}

type MockHandler = (args?: InvokeArgs) => unknown;

/** 演示会话内可变的签到自动化开关（切换 tab 后仍保留）。 */
let signinAutomation = { ...mockSigninAutomation };

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
      push(channel, { type: "initial", data: mockTodoItems });
      push(channel, { type: "snapshot", data: mockTodoItems });
    }
    return null;
  },
  refresh_todos: () => undefined,

  // 课表：与后端 initial/snapshot 事件形状一致。
  subscribe_timetable: (args) => {
    const channel = channelFrom(args);
    if (channel) {
      push(channel, { type: "initial", data: mockTimetableSnapshot });
      push(channel, { type: "snapshot", data: mockTimetableSnapshot });
    }
    return null;
  },
  refresh_timetable: () => undefined,

  // UOOC。
  get_uooc_snapshot: () => mockUoocSnapshot,
  refresh_uooc_progress: () => mockUoocSnapshot,
  subscribe_uooc_state: (args) => {
    const channel = channelFrom(args);
    if (channel) push(channel, { type: "snapshot", data: mockUoocSnapshot });
    return null;
  },
  get_uooc_ai_config: () => mockUoocAiConfig,
  save_uooc_ai_preferences: () => mockUoocAiConfig,
  get_ai_config: () => mockAiConfig,
  reset_ai_config: () => mockAiConfig,
  detect_ai_provider: () => ({
    protocol: "openAiCompatible",
    vendor: "openai",
    baseUrl: "https://api.example.org/v1",
    models: [
      {
        id: "demo-model",
        name: "Demo Model",
        url: "https://api.example.org/v1/chat/completions",
        toolCalling: true,
        vision: false,
        maxInputTokens: 119808,
        maxOutputTokens: 8192,
      },
    ],
    verified: true,
  }),
  preview_ai_model: (args) => {
    const model = String((args as any)?.input?.model ?? "demo-model");
    return {
      id: model,
      name: model,
      url: "https://api.example.org/v1/chat/completions",
      toolCalling: true,
      vision: false,
      maxInputTokens: 119808,
      maxOutputTokens: 8192,
    };
  },
  upsert_ai_provider: () => mockAiConfig,
  remove_ai_provider: () => mockAiConfig,
  list_ai_models: () => ["demo-model"],
  set_ai_default: () => mockAiConfig,
  get_debug_mode: () => ({ enabled: false }),
  get_uooc_local_bank_count: () => 12,
  get_uooc_embedding_status: () => ({
    status: "ready",
    cachedCount: 12,
    totalDimension: 512,
    updatedAtMs: 1,
    message: "",
  }),
  get_uooc_qa_account_status: () => ({
    isAnonymous: true,
    permissionLevel: "anonymous",
    canOverwrite: false,
  }),
  get_uooc_local_bank_questions: () => [],
  get_appearance: () => ({
    mode: "dark",
    seed: "#5d5fef",
    seedSource: "default",
    autoWallpaper: false,
    wallpaperPollMinutes: 15,
    variant: "tonal-spot",
  }),
  list_uooc_quizzes: () => mockUoocAssessments.homeworks ?? [],
  list_uooc_exams: () => mockUoocAssessments.exams ?? [],
  list_uooc_homeworks: () => mockUoocAssessments.homeworks ?? [],
  save_uooc_settings: () => mockUoocSnapshot,
  control_uooc_tasks: () => mockUoocSnapshot,
  start_uooc_checkin: () => mockUoocSnapshot,
  cancel_uooc_checkin: () => mockUoocSnapshot,
  refresh_uooc_session: () => mockUoocSnapshot,
  open_uooc_login: () => unavailable("open_uooc_login"),
  logout_uooc: () => undefined,
  get_sync_status: () => ({ enabled: false, items: [] }),
  subscribe_uooc_assessments: (args) => {
    const channel = channelFrom(args);
    if (channel) push(channel, { type: "initial", data: mockUoocAssessments });
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
  get_oj_paper: () => mockOjPaper,
  // OJ 页面主体走 Channel 订阅；无 IPC 时若不接管订阅，答题页会一直空着。
  subscribe_oj_state: (args) => {
    const channel = channelFrom(args);
    if (channel) {
      push(channel, { type: "initial", data: mockOjSnapshot });
      push(channel, { type: "snapshot", data: mockOjSnapshot });
    }
    return null;
  },
  subscribe_oj_lists: (args) => {
    const channel = channelFrom(args);
    if (channel) {
      // 键分别是站点 ID 与 `site:group`，与后端缓存口径一致。
      const siteId = mockOjSnapshot.sites[0]?.id ?? "";
      const groupId = mockOjGroups[0]?.id ?? "";
      push(channel, {
        type: "initial",
        data: {
          groups: { [siteId]: mockOjGroups },
          contests: { [`${siteId}:${groupId}`]: mockOjContests },
        },
      });
    }
    return null;
  },

  // 公共内容：公告与更新来自远端，浏览器里拉不到会让首页公告栏整块空掉。
  get_app_content_snapshot: () => mockAppContentSnapshot,
  refresh_app_content: () => mockAppContentSnapshot,
  // 公告头图、贡献者头像等经该命令取字节；无 IPC 时会落到「加载失败」占位。
  load_remote_resource: () => mockRemoteResourceBytes,

  // 畅课。
  get_lms_snapshot: () => mockLmsSnapshot,
  refresh_lms: () => mockLmsSnapshot,

  // 签到/打卡：演示模式给可交互的代表数据，提交静默成功（不会触达真实接口）。
  signin_get_buildings: () => mockSigninBuildings,
  signin_get_qw_status: () => mockSigninQwStatus,
  signin_get_timetable: () => mockSigninTimetable,
  signin_get_lms_rollcalls: () => mockSigninRollcalls,
  signin_resolve_radar_point: () => ({
    latitude: 22.528,
    longitude: 113.94,
    label: "致理楼",
    source: "fallback",
    fenceRadiusM: 130,
  }),
  signin_submit_lms_number: () => undefined,
  signin_submit_lms_radar: () => ({
    latitude: 22.528,
    longitude: 113.94,
    label: "致理楼",
    source: "fallback",
    fenceRadiusM: 130,
  }),
  signin_submit_lms_qr: () => undefined,
  signin_submit_szu: () => undefined,
  signin_get_last_photo: () => ({ hasPhoto: false }),
  signin_save_szu_photo: () => ({ hasPhoto: true }),
  signin_get_automation_settings: () => signinAutomation,
  signin_set_automation_settings: (args) => {
    const settings = (args as { settings?: Partial<typeof signinAutomation> } | undefined)
      ?.settings;
    if (settings) signinAutomation = { ...signinAutomation, ...settings };
    return signinAutomation;
  },
  signin_share_room_join: () => undefined,
  signin_share_room_leave: () => undefined,
  signin_share_room_share_qr: () => ({ shared: false }),
  signin_share_room_share_number: () => ({ shared: false }),
  subscribe_signin_share_events: () => undefined,

  // 巡课。
  get_jxxk_snapshot: () => mockJxxkSnapshot,
  refresh_jxxk: () => mockJxxkSnapshot,

  // 选课：单命令多 action，按 action 路由。
  // 只读查询给演示数据；真正会改动学校系统或需要真实会话的 action 仍拦截。
  enrollment_command: (args) => {
    const action = (args as { action?: unknown } | undefined)?.action;
    if (action === "snapshot" || action === "refresh") return mockEnrollmentSnapshot;
    if (typeof action === "object" && action !== null) {
      const type = (action as { type?: string }).type;
      if (type === "refresh" || type === "query") return mockEnrollmentSnapshot;
      if (type === "cacheSnapshot" || type === "cache") {
        return {
          ...mockEnrollmentSnapshot,
          complete: true,
          cachedPages: 1,
          total: mockEnrollmentSnapshot.courses.length,
        };
      }
      // 学校信息条（班次 / 学期徽标）在页面挂载后立刻请求，缺了会弹「操作失败」。
      if (type === "batchInfo") return { dataList: mockEnrollmentBatches };
      if (type === "selected") return mockEnrollmentSnapshot.selected;
    }
    if (action === "probe") {
      return [{ mode: "campus", latency: 12, needs2fa: false, message: "" }];
    }
    if (action === "session") return { status: "ok", snapshot: mockEnrollmentSnapshot };
    return unavailable(`enrollment_command(${String(action)})`);
  },
  // 在线人数由后端轮询推送；无 IPC 时给一个静态值，页面才不会显示「暂不可用」。
  watch_enrollment_online_users: (args) => {
    const channel = channelFrom(args);
    if (channel) push(channel, { online: 1284, updatedAt: Date.now(), error: null });
    return null;
  },
  subscribe_enrollment_state: (args) => {
    const channel = channelFrom(args);
    if (channel) push(channel, { type: "snapshot", data: mockEnrollmentSnapshot });
    return null;
  },
  open_webvpn_portal: () => unavailable("open_webvpn_portal"),
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
