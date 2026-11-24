import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LuBell, LuCalendarClock, LuMapPinCheck, LuRadio, LuTriangleAlert } from "react-icons/lu";
import { Button, Card, Checkbox, LoadingIndicator, Switch } from "material/react";
import { Banner, SettingList, SettingRow, toast } from "./ui";
import { errorMessage, useAsyncLoad } from "./async-state";
import { usePending } from "./use-pending";
import {
  DEFAULT_TODO_LEAD_MINUTES,
  NOTIFICATION_CATEGORIES,
  TODO_LEAD_PRESETS,
  TODO_NOTIFICATION_ITEMS,
} from "./notifications/categories";

export interface TodoNotificationConfig {
  enabled: boolean;
  discovery: boolean;
  ojQuiz: boolean;
  uoocExam: boolean;
  uoocHomework: boolean;
  uoocCourseEnd: boolean;
  lmsHomework: boolean;
  electricityOutage: boolean;
  leadMinutes: number[];
}

export interface TimetableNotificationConfig {
  classReminder: boolean;
  /** 实时活动（Android 16 promoted Live Update / ColorOS 流体云） */
  liveUpdate: boolean;
  checkin: boolean;
  checkinFailure: boolean;
}

export interface NotificationConfig {
  enabled: boolean;
  campusNetwork: boolean;
  enrollment: boolean;
  uooc: boolean;
  todo: TodoNotificationConfig;
  timetable: TimetableNotificationConfig;
}

const DEFAULT_TODO_CONFIG: TodoNotificationConfig = {
  enabled: true,
  discovery: true,
  ojQuiz: true,
  uoocExam: true,
  uoocHomework: true,
  uoocCourseEnd: true,
  lmsHomework: true,
  electricityOutage: true,
  leadMinutes: Array.from(DEFAULT_TODO_LEAD_MINUTES),
};

const DEFAULT_TIMETABLE_CONFIG: TimetableNotificationConfig = {
  classReminder: true,
  liveUpdate: true,
  checkin: true,
  checkinFailure: true,
};

const DEFAULT_CONFIG: NotificationConfig = {
  enabled: true,
  campusNetwork: true,
  enrollment: true,
  uooc: true,
  todo: DEFAULT_TODO_CONFIG,
  timetable: DEFAULT_TIMETABLE_CONFIG,
};

function normalizeConfig(saved?: Partial<NotificationConfig> | null): NotificationConfig {
  return {
    ...DEFAULT_CONFIG,
    ...saved,
    todo: {
      ...DEFAULT_TODO_CONFIG,
      ...saved?.todo,
      leadMinutes: saved?.todo?.leadMinutes ?? DEFAULT_TODO_CONFIG.leadMinutes,
    },
    timetable: { ...DEFAULT_TIMETABLE_CONFIG, ...saved?.timetable },
  };
}

/** 根级开关的键；`todo` / `timetable` 是嵌套对象，单独建模。 */
type RootKey = Exclude<keyof NotificationConfig, "todo" | "timetable">;
type TodoField = keyof TodoNotificationConfig;
type TimetableField = keyof TimetableNotificationConfig;

/**
 * 一次保存改动的字段。
 *
 * 失败后只回滚这个字段，而不是整份配置：同一时刻可能有另一个字段的保存已经成功，
 * 整份回滚会把那次成功的结果也抹掉，界面与服务端随即不一致。
 */
type Change =
  | { scope: "root"; key: RootKey }
  | { scope: "todo"; key: TodoField }
  | { scope: "timetable"; key: TimetableField };

function changeKey(change: Change) {
  if (change.scope === "todo") return `notify:todo.${change.key}`;
  if (change.scope === "timetable") return `notify:timetable.${change.key}`;
  return `notify:${change.key}`;
}

function patchRoot(config: NotificationConfig, key: RootKey, value: boolean): NotificationConfig {
  return { ...config, [key]: value };
}

function patchTodo<K extends TodoField>(
  config: NotificationConfig,
  key: K,
  value: TodoNotificationConfig[K],
): NotificationConfig {
  return { ...config, todo: { ...config.todo, [key]: value } };
}

function patchTimetable<K extends TimetableField>(
  config: NotificationConfig,
  key: K,
  value: TimetableNotificationConfig[K],
): NotificationConfig {
  return { ...config, timetable: { ...config.timetable, [key]: value } };
}

function revert(
  config: NotificationConfig,
  confirmed: NotificationConfig,
  change: Change,
): NotificationConfig {
  if (change.scope === "todo") {
    if (change.key === "leadMinutes") {
      return patchTodo(config, "leadMinutes", confirmed.todo.leadMinutes);
    }
    return patchTodo(config, change.key, confirmed.todo[change.key]);
  }
  if (change.scope === "timetable") {
    return patchTimetable(config, change.key, confirmed.timetable[change.key]);
  }
  return patchRoot(config, change.key, confirmed[change.key]);
}

function checkedOf(event: any): boolean {
  return Boolean(event.detail?.checked ?? event.target?.checked);
}

/**
 * 设置 → 通知：总开关、分类开关与待办提醒。
 *
 * 读取失败时**不能**退回默认配置：那样下一次开关会把默认值写回后端，覆盖用户
 * 真正保存的设置。因此读取状态用 `Async` 表达，失败分支只渲染重试横幅并把全部
 * 控件维持在禁用状态。保存也不再写进 `setConfig` 的 updater 里（那是在状态更新
 * 中做副作用），而是乐观更新草稿、串行提交、失败只回滚被改的那一个字段。
 */
export function NotificationSettings() {
  const pending = usePending();
  const { state, reload } = useAsyncLoad(async () =>
    normalizeConfig(await invoke<NotificationConfig | null>("get_notification_settings")),
  );
  const [config, setConfig] = useState<NotificationConfig | null>(null);
  const [permissionGranted, setPermissionGranted] = useState<boolean | null>(null);
  /** 最后一次确认落盘的配置，用于失败回滚。 */
  const confirmed = useRef<NotificationConfig | null>(null);
  /** 每个保存键最新一次期望写入的配置（双击时只保留最后一个）。 */
  const queued = useRef(new Map<string, NotificationConfig>());

  useEffect(() => {
    if (state.status !== "ready") return;
    confirmed.current = state.data;
    setConfig(state.data);
  }, [state]);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const { isPermissionGranted } = await import("@tauri-apps/plugin-notification");
        const granted = await isPermissionGranted();
        if (active) setPermissionGranted(granted);
      } catch {
        if (active) setPermissionGranted(true);
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  const loading = state.status === "idle" || state.status === "loading";
  const readOnly = loading || config === null;
  // 读取中/失败时用默认值把版式撑起来，但控件全部禁用。
  const view = config ?? DEFAULT_CONFIG;

  const requestPermission = async () => {
    const result = await pending.run("permission", async () => {
      const { isPermissionGranted, requestPermission } =
        await import("@tauri-apps/plugin-notification");
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      return granted;
    });
    if (!result.ok && result.skipped) return;
    if (!result.ok) {
      toast.danger("申请通知权限失败", { description: errorMessage(result.error) });
      return;
    }
    setPermissionGranted(result.value);
    if (result.value) {
      toast.success("已获得系统通知权限");
    } else {
      toast.warning("系统通知权限未开启", {
        description: "请在系统设置中允许本应用发送通知。",
      });
    }
  };

  /**
   * 提交一次改动：先乐观更新，同一键的写入串行，失败回滚该字段。
   *
   * 同键已有请求在跑时新值只进队列，由正在跑的那次循环接着发，因此快速连点
   * 不会交错，也不会用旧响应覆盖新状态。
   */
  const flush = async (key: string, change: Change) => {
    for (;;) {
      const next = queued.current.get(key);
      if (!next) return;
      const result = await pending.run(key, () =>
        invoke<NotificationConfig>("save_notification_settings", { settings: next }),
      );
      if (!result.ok && result.skipped) return;
      if (queued.current.get(key) === next) queued.current.delete(key);
      if (!result.ok) {
        queued.current.delete(key);
        const base = confirmed.current;
        if (base) setConfig((current) => (current ? revert(current, base, change) : current));
        toast.danger("保存通知设置失败", { description: errorMessage(result.error) });
        return;
      }
      const saved = normalizeConfig(result.value);
      confirmed.current = saved;
      if (queued.current.has(key)) continue;
      setConfig((current) => (current === next ? saved : current));
      return;
    }
  };

  const persist = (change: Change, next: NotificationConfig) => {
    if (config === null) return;
    const key = changeKey(change);
    setConfig(next);
    queued.current.set(key, next);
    void flush(key, change);
  };

  const toggleRoot = (key: RootKey, checked: boolean) => {
    if (readOnly || !config) return;
    persist({ scope: "root", key }, patchRoot(config, key, checked));
  };

  const toggleTodoFlag = (key: Exclude<TodoField, "leadMinutes">, checked: boolean) => {
    if (readOnly || !config) return;
    persist({ scope: "todo", key }, patchTodo(config, key, checked));
  };

  const toggleTimetableFlag = (key: TimetableField, checked: boolean) => {
    if (readOnly || !config) return;
    persist({ scope: "timetable", key }, patchTimetable(config, key, checked));
  };

  const toggleLeadMinutes = (minutes: number, checked: boolean) => {
    if (readOnly || !config) return;
    const next = checked
      ? [...config.todo.leadMinutes, minutes]
      : config.todo.leadMinutes.filter((value) => value !== minutes);
    persist(
      { scope: "todo", key: "leadMinutes" },
      patchTodo(
        config,
        "leadMinutes",
        Array.from(new Set(next)).sort((a, b) => b - a),
      ),
    );
  };

  const categoryDisabled = readOnly || !view.enabled;
  const todoDisabled = categoryDisabled || !view.todo.enabled;

  return (
    <>
      {loading && (
        <p className="status-line">
          <LoadingIndicator size={16} aria-label="正在读取通知设置" />
          正在读取通知设置…
        </p>
      )}

      {state.status === "error" && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>读取通知设置失败</Banner.Title>
            <Banner.Description>
              <span className="block">{state.message}</span>
              <span className="block">
                为避免把默认值写回，所有开关暂时禁用；重试成功前不会保存任何改动。
              </span>
            </Banner.Description>
          </Banner>
          <Button variant="outlined" onClick={() => void reload()}>
            重试
          </Button>
        </div>
      )}

      {permissionGranted === false && (
        <div className="grid justify-items-start gap-3">
          <Banner status="warning">
            <Banner.Title>系统通知权限未开启</Banner.Title>
            <Banner.Description>
              后台运行时将无法接收系统推送通知；前台聚焦使用时仍会通过应用内 Snackbar 提示。
            </Banner.Description>
          </Banner>
          <Button
            variant="outlined"
            disabled={pending.isPending("permission")}
            onClick={() => void requestPermission()}
          >
            授权系统通知
          </Button>
        </div>
      )}

      <Card variant="outlined">
        <SettingRow
          icon={<LuBell aria-hidden="true" />}
          title="通知总开关"
          description="关闭后，所有系统通知和前台应用内提示均被静音。"
        >
          <Switch
            checked={view.enabled}
            disabled={readOnly}
            onChange={(event: any) => toggleRoot("enabled", checkedOf(event))}
            aria-label="启用所有通知"
          />
        </SettingRow>
      </Card>

      <Card variant="outlined">
        <div slot="headline">通知分类管理</div>
        <div slot="subhead">
          根据需要开启或关闭特定业务的通知提醒。应用在前台聚焦状态下将自动以 Snackbar
          展示，后台运行时弹出系统通知。
        </div>
        <SettingList>
          {NOTIFICATION_CATEGORIES.map((cat) => {
            const Icon = cat.icon;
            return (
              <SettingRow
                key={cat.configKey}
                icon={<Icon aria-hidden="true" />}
                title={cat.title}
                description={cat.description}
              >
                <Switch
                  checked={view[cat.configKey]}
                  disabled={categoryDisabled}
                  onChange={(event: any) => toggleRoot(cat.configKey, checkedOf(event))}
                  aria-label={`开关${cat.title}通知`}
                />
              </SettingRow>
            );
          })}
        </SettingList>
      </Card>

      <Card variant="outlined">
        <div slot="headline">上课提醒</div>
        <div slot="subhead">
          课表模块的通知。课前提醒从上课前 20 分钟起显示常驻倒计时（Android），上课即收起。
        </div>
        <SettingList>
          <SettingRow
            icon={<LuCalendarClock aria-hidden="true" />}
            title="课前提醒"
            description="每节课开始前 20 分钟提醒，包含课程名、时间段与地点。"
          >
            <Switch
              checked={view.timetable.classReminder}
              disabled={categoryDisabled}
              onChange={(event: any) => toggleTimetableFlag("classReminder", checkedOf(event))}
              aria-label="开关课前提醒"
            />
          </SettingRow>
          <SettingRow
            icon={<LuRadio aria-hidden="true" />}
            title="实时活动（流体云）"
            description="课前 20 分钟起以实时活动展示进度环与倒计时，上课即收起；退到后台也会按分钟更新（Android 16 实时更新；ColorOS 流体云据此渲染）。需要系统的实时通知授权。"
          >
            <Switch
              checked={view.timetable.liveUpdate}
              disabled={categoryDisabled || !view.timetable.classReminder}
              onChange={(event: any) => toggleTimetableFlag("liveUpdate", checkedOf(event))}
              aria-label="开关课前实时活动"
            />
          </SettingRow>
          <SettingRow
            icon={<LuMapPinCheck aria-hidden="true" />}
            title="签到提醒"
            description="探测到新的点名待签时提醒，点击前往签到页。"
          >
            <Switch
              checked={view.timetable.checkin}
              disabled={categoryDisabled}
              onChange={(event: any) => toggleTimetableFlag("checkin", checkedOf(event))}
              aria-label="开关签到提醒"
            />
          </SettingRow>
          <SettingRow
            icon={<LuTriangleAlert aria-hidden="true" />}
            title="签到/打卡失败提醒"
            description="自动签到失败、缺少照片被跳过或需要手动处理时提醒。"
          >
            <Switch
              checked={view.timetable.checkinFailure}
              disabled={categoryDisabled}
              onChange={(event: any) => toggleTimetableFlag("checkinFailure", checkedOf(event))}
              aria-label="开关签到失败提醒"
            />
          </SettingRow>
        </SettingList>
      </Card>

      <Card variant="outlined">
        <div slot="headline">待办提醒</div>
        <div slot="subhead">
          统一管理首页日历中全部待办事项的提醒（初次探测、任务开始与截止前）。系统通知在应用后台运行时发送，前台聚焦时以
          Snackbar 展示。
        </div>
        <SettingList>
          <SettingRow
            title="启用待办提醒"
            description="关闭后，所有待办都不再按事件与截止时间发送通知。"
          >
            <Switch
              checked={view.todo.enabled}
              disabled={categoryDisabled}
              onChange={(event: any) => toggleTodoFlag("enabled", checkedOf(event))}
              aria-label="启用待办提醒"
            />
          </SettingRow>
          {TODO_NOTIFICATION_ITEMS.map((item) => {
            const Icon = item.icon;
            return (
              <SettingRow
                key={item.key}
                icon={<Icon aria-hidden="true" />}
                title={item.title}
                description={item.description}
              >
                <Switch
                  checked={view.todo[item.key]}
                  disabled={todoDisabled}
                  onChange={(event: any) => toggleTodoFlag(item.key, checkedOf(event))}
                  aria-label={`开关${item.title}提醒`}
                />
              </SettingRow>
            );
          })}
        </SettingList>

        <div className="card-group mt-4">
          <div className="grid gap-1">
            <p className="card-group-title">提醒时机</p>
            <p className="text-xs text-muted">
              待办首次出现时可提醒一次；任务开始时始终会再提醒一次；之后在每个已勾选的时间点各提醒一次。应用离线期间错过的多个时间点只补发最接近截止的一档。
            </p>
          </div>
          <div className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3">
            <Checkbox
              checked={view.todo.discovery}
              disabled={todoDisabled}
              onChange={(event: any) => toggleTodoFlag("discovery", checkedOf(event))}
            >
              初次探测到时
            </Checkbox>
            <Checkbox checked disabled>
              任务开始时
            </Checkbox>
            {TODO_LEAD_PRESETS.map((preset) => (
              <Checkbox
                key={preset.minutes}
                checked={view.todo.leadMinutes.includes(preset.minutes)}
                disabled={todoDisabled}
                onChange={(event: any) => toggleLeadMinutes(preset.minutes, checkedOf(event))}
              >
                {preset.label}
              </Checkbox>
            ))}
          </div>
          {!readOnly &&
            view.todo.enabled &&
            view.todo.leadMinutes.length === 0 &&
            (view.todo.discovery ? (
              <Banner status="info">
                <span>
                  未勾选任何提前提醒时间，待办将只在「初次探测到时」与「任务开始时」提醒。
                </span>
              </Banner>
            ) : (
              <Banner status="info">
                <span>未勾选任何提前提醒时间，待办将只在「任务开始时」提醒。</span>
              </Banner>
            ))}
        </div>
      </Card>
    </>
  );
}
