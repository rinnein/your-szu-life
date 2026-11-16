import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LoadingIndicator, Button } from "material/react";
import { Banner, toast } from "./ui";
import { LuWifi } from "react-icons/lu";
import { Card } from "./components";
import type { NetworkFeePortalNotice } from "./campus-payments";

export default function CampusPaymentsNetworkPage() {
  const [busy, setBusy] = useState(false);
  /** 官方门户自动化上次留下的持久提示；打开失败走 toast，不占用这里。 */
  const [notice, setNotice] = useState<string>();
  const opening = useRef(false);
  const noticeSuppressed = useRef(false);
  const noticeRequest = useRef<Promise<NetworkFeePortalNotice | null> | undefined>(undefined);

  useEffect(() => {
    noticeRequest.current ??= invoke<NetworkFeePortalNotice | null>(
      "consume_network_fee_portal_notice",
    );
    void noticeRequest.current
      .then((x) => x && !noticeSuppressed.current && setNotice(x.message))
      .catch(() => {
        if (noticeSuppressed.current) return;
        setNotice("未能读取上次网费办理提示，可重新打开官网");
      });
  }, []);

  const open = async () => {
    if (opening.current) return;
    opening.current = true;
    noticeSuppressed.current = true;
    setBusy(true);
    try {
      await invoke("open_network_fee_portal");
    } catch {
      toast.danger("无法打开网费官网，请检查网络或重新认证后重试");
    } finally {
      setBusy(false);
      opening.current = false;
    }
  };

  return (
    <Card
      title="网费"
      icon={<LuWifi />}
      description="在独立窗口打开官网，必要时使用已保存的校园卡号和统一认证密码辅助登录"
    >
      <div className="grid gap-4">
        {notice && (
          <Banner status="warning">
            <Banner.Title>网费办理提示</Banner.Title>
            <Banner.Description>{notice}</Banner.Description>
          </Banner>
        )}

        <section
          className="grid gap-2.5 p-4 rounded-inner bg-accent-soft leading-relaxed"
          aria-label="网费办理说明"
        >
          <p>请先在校园卡「通用代扣费账户」充值足够金额</p>
          <p>
            若当前套餐仍剩余多个月，请在套餐剩余一个月内预订下个月套餐；只有无套餐或套餐剩余一个月内，才能修改或续费套餐
          </p>
        </section>

        <Button variant="filled" disabled={busy} onClick={() => void open()}>
          {busy && <LoadingIndicator size={16} />}
          {busy ? "正在打开网费官网…" : "打开网费官网"}
        </Button>

        <p className="text-sm text-muted leading-relaxed">
          每次打开最多自动提交一次登录；遇到验证码或登录错误将停止自动操作。办理时可随时返回本页
        </p>
      </div>
    </Card>
  );
}
