/**
 * 畅课签到面板：待签列表 + 交互对话框（数字 / 雷达 / 扫码）。
 *
 * 列表读取用 Async 可辨识联合（读取失败呈现错误态而不是空态）；二维码
 * 房间状态提升到本面板，对话框关闭后「匹配中」仍在（见 qr-state.ts）。
 * 刷新动作在页面顶栏：refreshNonce 递增触发强制重取，busy 由本面板回报。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as tauriCore from "@tauri-apps/api/core";
import { Button, Card, Switch } from "material/react";
import { LuRadar } from "react-icons/lu";
import { Banner, Chip, SettingList, SettingRow } from "../ui";
import { EmptyState, LoadingState } from "../components";
import { useLatestRequest } from "../async-state";
import { SigninList, SigninRow } from "./SigninList";
import { LmsRollcallDialog } from "./LmsRollcallDialog";
import { useShareRoom } from "./use-share-room";
import { useSigninAutomation } from "./use-signin-automation";
import {
  needAnswer,
  ROLLCALL_METHOD_LABEL,
  rollcallMethod,
  rollcallStatusLabel,
  type RollcallItem,
} from "./types";
import type { Building } from "./types";

interface LmsPanelProps {
  buildings: Building[];
  buildingsFailed?: boolean;
  initialCourse?: string;
  open?: boolean;
  refreshNonce: number;
  onBusyChange: (busy: boolean) => void;
}

const normalizeCourse = (value: string | undefined) =>
  (value ?? "").replace(/\s/g, "").toLowerCase();

const checkedOf = (event: any): boolean => Boolean(event.detail?.checked ?? event.target?.checked);

export function LmsPanel({
  buildings,
  buildingsFailed,
  initialCourse,
  open,
  refreshNonce,
  onBusyChange,
}: LmsPanelProps) {
  const forceRef = useRef(false);
  const { state, run } = useLatestRequest<RollcallItem[]>(() =>
    tauriCore.invoke<RollcallItem[]>("signin_get_lms_rollcalls", { force: forceRef.current }),
  );
  const [active, setActive] = useState<RollcallItem | null>(null);
  const share = useShareRoom();
  const automation = useSigninAutomation();

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

  const items =
    state.status === "ready"
      ? state.data
      : state.status === "loading" || state.status === "error"
        ? (state.previous ?? [])
        : [];
  const pendingItems = useMemo(() => items.filter(needAnswer), [items]);
  const signedItems = useMemo(() => items.filter((item) => !needAnswer(item)), [items]);

  // 深链接：open=1 且课程匹配时自动打开对应点名（只触发一次）。
  const opened = useRef(false);
  useEffect(() => {
    if (!open || opened.current || !initialCourse || state.status !== "ready") return;
    const target = pendingItems.find(
      (item) => normalizeCourse(item.course_title) === normalizeCourse(initialCourse),
    );
    if (target) {
      opened.current = true;
      setActive(target);
    }
  }, [open, initialCourse, state.status, pendingItems]);

  const closeDialog = () => {
    if (active) {
      // 关闭承载共享/确认页的对话框时回收本地扫码循环；房间保留给被动匹配。
      if (share.state.stage === "sharing" && share.state.courseId === active.course_id) {
        share.dialogClosed(active.course_id);
      } else if (
        share.state.stage === "confirmShare" &&
        share.state.courseId === active.course_id
      ) {
        share.cancelSharePrompt(active.course_id);
      }
    }
    setActive(null);
  };

  const initialLoading = state.status === "idle" || (state.status === "loading" && !state.previous);
  const loadFailed = state.status === "error" && !state.previous;

  return (
    <section aria-label="畅课待签到列表" className="flex flex-col gap-4">
      <Card variant="outlined">
        <div slot="headline">自动雷达签到</div>
        <div slot="subhead">
          检测到畅课雷达点名时，按「上次成功定位 → 课表楼栋 → 兜底楼栋」自动提交。
          默认关闭；仅在应用进程存活时运行，锁屏或系统回收进程时可能错过。
        </div>
        <SettingList>
          <SettingRow
            icon={<LuRadar aria-hidden="true" />}
            title="启用自动雷达签到"
            description="只处理雷达类点名；数字与扫码仍需手动完成。"
          >
            <Switch
              checked={automation.settings.lmsAutoRadar}
              disabled={
                automation.loading ||
                automation.failed ||
                automation.pending.isPending("automation")
              }
              onChange={(event: any) => automation.toggle("lmsAutoRadar", checkedOf(event))}
              aria-label="开关畅课自动雷达签到"
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
      {loadFailed ? (
        <EmptyState
          variant="error"
          title="无法读取待签到列表"
          description={state.message}
          onRetry={() => load(false)}
        />
      ) : initialLoading ? (
        <LoadingState message="正在读取待签到列表…" />
      ) : (
        <>
          {pendingItems.length > 0 && (
            <SigninList label="待签到项">
              {pendingItems.map((item) => {
                const matching =
                  share.state.stage === "matching" && share.state.courseId === item.course_id;
                return (
                  <SigninRow
                    key={item.rollcall_id}
                    title={item.course_title || `课程 ${item.course_id}`}
                    onClick={() => setActive(item)}
                    trailing={
                      <>
                        {matching && (
                          <Chip size="sm" color="info" variant="soft">
                            匹配中
                          </Chip>
                        )}
                        <Chip size="sm" color="accent" variant="soft">
                          {ROLLCALL_METHOD_LABEL[rollcallMethod(item)]}
                        </Chip>
                      </>
                    }
                  />
                );
              })}
            </SigninList>
          )}
          {signedItems.length > 0 && (
            <div className="flex flex-col gap-2">
              <h2 className="text-sm font-medium text-muted">已签到</h2>
              <SigninList label="已签到">
                {signedItems.map((item) => (
                  <SigninRow
                    key={item.rollcall_id}
                    title={item.course_title || `课程 ${item.course_id}`}
                    trailing={
                      <Chip size="sm" color="success" variant="soft">
                        {rollcallStatusLabel(item.status)}
                      </Chip>
                    }
                  />
                ))}
              </SigninList>
            </div>
          )}
          {pendingItems.length === 0 && signedItems.length === 0 && (
            <EmptyState
              title="当前没有待签到的点名"
              description="课程开始后会自动探测并在这里出现。"
            />
          )}
        </>
      )}

      {active && (
        <LmsRollcallDialog
          item={active}
          buildings={buildings}
          buildingsFailed={buildingsFailed}
          shareState={share.state}
          shareActions={share}
          onClose={closeDialog}
          onSubmitted={() => {
            setActive(null);
            load(true);
          }}
        />
      )}
    </section>
  );
}
