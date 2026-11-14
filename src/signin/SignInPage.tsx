/**
 * 签到/打卡页：畅课（LMS）与 SZU（实验性）两个签到渠道。
 */
import { useEffect, useState } from "react";
import * as tauriCore from "@tauri-apps/api/core";
import { AdaptiveTabs } from "../ui";
import { useLogicalNavigation } from "../logical-navigation";
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

export default function SignInPage({ tab }: { tab: "lms" | "szu" }) {
  const navigation = useLogicalNavigation();
  const [buildings, setBuildings] = useState<Building[]>([]);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const list = await tauriCore.invoke<Building[]>("signin_get_buildings");
        if (active) setBuildings(list);
      } catch {
        // 地点列表加载失败不阻塞页面；下拉会为空，提交走默认解析链。
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar title="签到打卡" description="畅课与 SZU 考勤的数字、雷达、扫码与拍照签到" />

        <AdaptiveTabs
          label="签到渠道"
          items={TAB_ITEMS}
          value={tab}
          onChange={(next) => void navigation.replace(`/study/signin?tab=${next}`)}
          ariaLabel="签到渠道"
        />

        {tab === "lms" ? <LmsPanel buildings={buildings} /> : <SzuPanel buildings={buildings} />}
      </PageSurface>
    </AuthGate>
  );
}
