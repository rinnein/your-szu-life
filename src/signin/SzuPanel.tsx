/**
 * SZU（实验性）签到面板：当天课次列表 + 拍照签到对话框。
 *
 * 会话状态与课表合成一次读取，返回可辨识联合：needs_auth / unavailable
 * 只出 Banner（附刷新），不再与 toast 双重提示；读取失败呈现错误态而
 * 不是空态。刷新动作在页面顶栏（refreshNonce / onBusyChange）。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import * as tauriCore from "@tauri-apps/api/core";
import { Button, Card, Switch } from "material/react";
import { LuMapPinCheck } from "react-icons/lu";
import { Banner, SettingList, SettingRow } from "../ui";
import { EmptyState, LoadingState } from "../components";
import { useLatestRequest } from "../async-state";
import { SigninList, SigninRow } from "./SigninList";
import { SzuSigninDialog } from "./SzuSigninDialog";
import { useSigninAutomation } from "./use-signin-automation";
import {
  formatSessionTime,
  localDateKey,
  type TimetableCache,
  type TimetableSession,
} from "./types";
import type { Building } from "./types";

/** 一次读取的快照：考勤会话状态与当天课次合成为一个可辨识联合。 */
type SzuSnapshot =
  | { kind: "ready"; sessions: TimetableSession[] }
  | { kind: "needs_auth"; message: string }
  | { kind: "unavailable"; message: string };

const checkedOf = (event: any): boolean => Boolean(event.detail?.checked ?? event.target?.checked);

interface SzuPanelProps {
  buildings: Building[];
  buildingsFailed?: boolean;
  initialCourse?: string;
  initialStart?: number;
  open?: boolean;
  refreshNonce: number;
  onBusyChange: (busy: boolean) => void;
}

export function SzuPanel({
  buildings,
  buildingsFailed,
  initialCourse,
  initialStart,
  open,
  refreshNonce,
  onBusyChange,
}: SzuPanelProps) {
  const forceRef = useRef(false);
  const automation = useSigninAutomation();
  const { state, run } = useLatestRequest<SzuSnapshot>(async () => {
    const status = await tauriCore.invoke<{
      state: "ready" | "needs_auth" | "unavailable";
      message?: string;
    }>("signin_get_qw_status", { force: forceRef.current });
    if (status.state !== "ready") {
      return { kind: status.state, message: status.message ?? "" };
    }
    const cache = await tauriCore.invoke<TimetableCache>("signin_get_timetable", {
      force: forceRef.current,
    });
    const today = localDateKey();
    return {
      kind: "ready",
      sessions: cache.sessions.filter((session) => session.date === today),
    };
  });
  const [active, setActive] = useState<TimetableSession | null>(null);

  const load = useCallback(
    (force: boolean) => {
      forceRef.current = force;
      void run();
    },
    [run],
  );

  useEffect(() => {
    load(false);
  }, [load]);
  useEffect(() => {
    if (refreshNonce > 0) load(true);
  }, [refreshNonce, load]);

  // 顶栏刷新按钮的进行态；卸载时复位。
  useEffect(() => {
    onBusyChange(state.status === "loading");
    return () => onBusyChange(false);
  }, [state.status, onBusyChange]);

  // 深链接：open=1 且课程+开始时间匹配时自动打开对应课次（只触发一次）。
  const opened = useRef(false);
  const sessions =
    state.status === "ready" && state.data.kind === "ready" ? state.data.sessions : [];
  useEffect(() => {
    if (!open || opened.current || !initialCourse || !initialStart || state.status !== "ready") {
      return;
    }
    const target = sessions.find(
      (session) => session.course_name === initialCourse && session.start_ms === initialStart,
    );
    if (target) {
      opened.current = true;
      setActive(target);
    }
  }, [open, initialCourse, initialStart, state.status, sessions]);

  const initialLoading = state.status === "idle" || (state.status === "loading" && !state.previous);
  const loadFailed = state.status === "error" && !state.previous;
  const snapshot =
    state.status === "ready"
      ? state.data
      : state.status === "loading" || state.status === "error"
        ? (state.previous ?? null)
        : null;

  return (
    <section aria-label="SZU 考勤课次列表" className="flex flex-col gap-4">
      <Card variant="outlined">
        <div slot="headline">签到提醒与自动签到</div>
        <div slot="subhead">
          开启后，检测到 SZU 考勤可签到的课次时自动用雷达定位提交；无法自动完成时发送签到提醒。
          默认关闭，且仍受「设置 → 通知 → 签到提醒」开关约束。
        </div>
        <SettingList>
          <SettingRow
            icon={<LuMapPinCheck aria-hidden="true" />}
            title="启用签到提醒与自动签到"
            description="自动签到仅在应用进程存活时运行，锁屏或系统回收进程时可能错过。"
          >
            <Switch
              checked={automation.settings.szuEnabled}
              disabled={
                automation.loading ||
                automation.failed ||
                automation.pending.isPending("automation")
              }
              onChange={(event: any) => automation.toggle("szuEnabled", checkedOf(event))}
              aria-label="开关 SZU 签到提醒与自动签到"
            />
          </SettingRow>
        </SettingList>
      </Card>
      {automation.failed && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>无法读取自动签到设置</Banner.Title>
            <Banner.Description>
              {automation.state.status === "error" ? automation.state.message : ""}
            </Banner.Description>
          </Banner>
          <Button variant="outlined" onClick={() => void automation.reload()}>
            重试
          </Button>
        </div>
      )}
      {automation.settings.szuEnabled && (
        <Banner status="warning">
          <Banner.Title>自动签到需要提前为课程配置照片</Banner.Title>
          <Banner.Description>
            <span className="block">
              没有保存照片的课程会被跳过，并发送失败提醒。请打开课次，选择或拍摄照片后点「保存照片」；
              保存不要求签到成功，照片会按课程记忆供后续自动签到使用。
            </span>
            <span className="block">
              自动签到仅在应用进程存活时运行；锁屏或系统回收进程时可能错过。
            </span>
          </Banner.Description>
        </Banner>
      )}
      {loadFailed ? (
        <EmptyState
          variant="error"
          title="无法读取 SZU 考勤状态"
          description={state.message}
          onRetry={() => load(false)}
        />
      ) : initialLoading ? (
        <LoadingState message="正在读取 SZU 考勤状态…" />
      ) : snapshot && snapshot.kind !== "ready" ? (
        <>
          <Banner status="warning">
            <Banner.Title>
              {snapshot.kind === "needs_auth"
                ? "尚未获取到 SZU 考勤会话"
                : "SZU 考勤服务暂时不可用"}
            </Banner.Title>
            <Banner.Description>
              {snapshot.message ||
                (snapshot.kind === "needs_auth"
                  ? "请先在应用内完成深圳大学统一认证，然后点击刷新。"
                  : "网络异常，稍后可点击刷新重试。")}
            </Banner.Description>
          </Banner>
          <Button variant="tonal" className="self-start" onClick={() => load(true)}>
            刷新
          </Button>
        </>
      ) : snapshot && snapshot.sessions.length === 0 ? (
        <EmptyState
          title="今天没有课次"
          description="课表来自 SZU 考勤系统的缓存（6 小时刷新一次）。"
        />
      ) : snapshot ? (
        <SigninList label="今天的课次">
          {snapshot.sessions.map((session, index) => (
            <SigninRow
              key={`${session.course_name}-${session.start_ms}-${index}`}
              title={session.course_name}
              supporting={[session.building_name ?? "地点未知", session.classroom]
                .filter(Boolean)
                .join(" · ")}
              trailing={
                <span className="text-sm text-muted whitespace-nowrap">
                  {formatSessionTime(session.start_ms, session.end_ms)}
                </span>
              }
              onClick={() => setActive(session)}
            />
          ))}
        </SigninList>
      ) : null}

      {active && (
        <SzuSigninDialog
          session={active}
          buildings={buildings}
          buildingsFailed={buildingsFailed}
          onClose={() => setActive(null)}
        />
      )}
    </section>
  );
}
