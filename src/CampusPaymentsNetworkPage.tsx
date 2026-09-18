import { useEffect, useRef, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Spinner } from "@heroui/react";
import { LuWifi } from "react-icons/lu";
import { useAuth } from "./auth";
import type { NetworkFeePortalNotice } from "./campus-payments";

export default function CampusPaymentsNetworkPage() {
  const { state } = useAuth();
  const navigate = useNavigate();
  const authenticated = "Authenticated" in state;
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string>();
  const opening = useRef(false);
  const noticeSuppressed = useRef(false);
  const noticeRequest = useRef<Promise<NetworkFeePortalNotice | null> | undefined>(undefined);
  useEffect(() => {
    if (!authenticated) void navigate({ to: "/", replace: true });
  }, [authenticated, navigate]);
  useEffect(() => {
    if (!authenticated) return;
    noticeRequest.current ??= invoke<NetworkFeePortalNotice | null>(
      "consume_network_fee_portal_notice",
    );
    void noticeRequest.current
      .then((x) => x && !noticeSuppressed.current && setNotice(x.message))
      .catch(() => setNotice("未能读取上次网费办理提示，可重新打开官网。"));
  }, [authenticated]);
  const open = async () => {
    if (opening.current) return;
    opening.current = true;
    noticeSuppressed.current = true;
    setBusy(true);
    setNotice(undefined);
    try {
      await invoke("open_network_fee_portal");
    } catch {
      setNotice("无法打开网费官网，请检查网络或重新认证后重试。");
      setBusy(false);
      opening.current = false;
    }
  };
  if (!authenticated) return null;
  return (
    <Card className="payments-card" aria-labelledby="network-fee-heading">
      <Card.Header>
        <Card.Title id="network-fee-heading" className="payments-card-title">
          <LuWifi aria-hidden="true" />
          网费
        </Card.Title>
        <Card.Description>
          在当前窗口打开官网，必要时使用已保存的校园卡号和统一认证密码辅助登录。
        </Card.Description>
      </Card.Header>
      <Card.Content className="payments-network-content">
        {notice && (
          <Alert status="warning" role="alert">
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Title>网费办理提示</Alert.Title>
              <Alert.Description>{notice}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        <section className="payments-guidance" aria-label="网费办理说明">
          <p>请先在校园卡「通用代扣费账户」充值足够金额。</p>
          <p>
            若当前套餐仍剩余多个月，请在套餐剩余一个月内预订下个月套餐；只有无套餐或套餐剩余一个月内，才能修改或续费套餐。
          </p>
        </section>
        <Button variant="primary" isDisabled={busy} onPress={() => void open()}>
          {busy && <Spinner size="sm" color="current" />}
          {busy ? "正在打开网费官网…" : "打开网费官网"}
        </Button>
        <p className="payments-hint">
          每次打开最多自动提交一次登录；遇到验证码或登录错误将停止自动操作。办理时可随时返回本页。
        </p>
      </Card.Content>
    </Card>
  );
}
