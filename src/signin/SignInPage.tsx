/**
 * 签到/打卡页：畅课（LMS）与 SZU（实验性）两个签到渠道。
 *
 * 刷新收在顶栏动作区（进行中换成指示器并禁用），busy 由当前面板回报；
 * 楼栋列表读取失败不阻塞页面，下拉只剩默认项并给一行说明。
 */
import { useCallback, useState } from "react";
import * as tauriCore from "@tauri-apps/api/core";
import { Button, LoadingIndicator } from "material/react";
import { LuRefreshCw } from "react-icons/lu";
import { AdaptiveTabs } from "../ui";
import { useLogicalNavigation } from "../logical-navigation";
import { useAsyncLoad } from "../async-state";
import { PageSurface } from "../shell/PageSurface";
import { TopAppBar } from "../shell/TopAppBar";
import { AuthGate } from "../shell/AuthGate";
import { LmsPanel } from "./LmsPanel";
import { SzuPanel } from "./SzuPanel";
import type { Building } from "./types";

const TAB_ITEMS = [
  { value: "lms", label: "畅课" },
  { value: "szu", label: "SZU（实验性）" },
] as const;

export default function SignInPage({
  tab,
  course,
  start,
  open,
}: {
  tab: "lms" | "szu";
  course?: string;
  start?: number;
  open?: boolean;
}) {
  const navigation = useLogicalNavigation();

  const loadedBuildings = useAsyncLoad<Building[]>(() =>
    tauriCore.invoke<Building[]>("signin_get_buildings"),
  );
  const buildings = loadedBuildings.state.status === "ready" ? loadedBuildings.state.data : [];
  const buildingsFailed = loadedBuildings.state.status === "error";

  const [refresh, setRefresh] = useState<{ nonce: number; busy: boolean }>({
    nonce: 0,
    busy: false,
  });
  const requestRefresh = useCallback(
    () => setRefresh((r) => ({ nonce: r.nonce + 1, busy: true })),
    [],
  );
  const setBusy = useCallback(
    (busy: boolean) => setRefresh((r) => (r.busy === busy ? r : { ...r, busy })),
    [],
  );

  return (
    <AuthGate>
      <PageSurface maxWidth="md">
        <TopAppBar
          title="签到打卡"
          description="畅课与 SZU 考勤的数字、雷达、扫码与拍照签到"
          actions={
            <Button
              variant="text"
              disabled={refresh.busy}
              onClick={requestRefresh}
              aria-label="刷新签到信息"
            >
              {refresh.busy ? <LoadingIndicator size={16} /> : <LuRefreshCw aria-hidden="true" />}
              刷新
            </Button>
          }
        />

        <div className="grid gap-6">
          <AdaptiveTabs
            label="签到渠道"
            items={TAB_ITEMS}
            value={tab}
            onChange={(next) => void navigation.replace(`/study/signin?tab=${next}`)}
            ariaLabel="签到渠道"
          />

          {tab === "lms" ? (
            <LmsPanel
              buildings={buildings}
              buildingsFailed={buildingsFailed}
              initialCourse={course}
              open={open}
              refreshNonce={refresh.nonce}
              onBusyChange={setBusy}
            />
          ) : (
            <SzuPanel
              buildings={buildings}
              buildingsFailed={buildingsFailed}
              initialCourse={course}
              initialStart={start}
              open={open}
              refreshNonce={refresh.nonce}
              onBusyChange={setBusy}
            />
          )}
        </div>
      </PageSurface>
    </AuthGate>
  );
}
