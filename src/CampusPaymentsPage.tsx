import { useEffect, useRef, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, CloseButton, Spinner } from "@heroui/react";
import { QRCodeSVG } from "qrcode.react";
import { LuArrowLeft, LuCopy, LuCreditCard, LuWifi, LuZap } from "react-icons/lu";
import { PageShell } from "./components";
import { useAuth } from "./auth";
import { copyText } from "./clipboard";
import { WECHAT_SERVICES, type NetworkFeePortalNotice } from "./campus-payments";

function WeChatServiceCard({ service }: { service: (typeof WECHAT_SERVICES)[number] }) {
  const [copying, setCopying] = useState(false);
  const [message, setMessage] = useState<string>();
  const Icon = service.id === "electricity" ? LuZap : LuCreditCard;

  const copyLink = async () => {
    setCopying(true);
    setMessage(undefined);
    const copied = await copyText(service.url);
    setMessage(
      copied
        ? "链接已复制，请在微信内粘贴打开。"
        : "无法自动复制，请长按或选择下方完整链接手动复制。",
    );
    setCopying(false);
  };

  return (
    <Card className="payments-card" aria-labelledby={service.id + "-heading"}>
      <Card.Header>
        <Card.Title id={service.id + "-heading"} className="payments-card-title">
          <Icon aria-hidden="true" />
          {service.title}
        </Card.Title>
        <Card.Description>{service.description}</Card.Description>
      </Card.Header>
      <Card.Content className="payments-wechat-content">
        <p className="payments-instruction">复制链接后在微信内打开，或使用微信扫码</p>
        <Button
          variant="secondary"
          aria-label={"复制" + service.title + "链接"}
          isDisabled={copying}
          onPress={() => void copyLink()}
        >
          <LuCopy aria-hidden="true" />
          {copying ? "正在复制…" : "复制链接"}
        </Button>
        {message && (
          <p className="payments-copy-message" role="status">
            {message}
          </p>
        )}
        <code className="payments-link" tabIndex={0} aria-label={service.title + "完整链接"}>
          {service.url}
        </code>
        <figure className="payments-qr">
          <QRCodeSVG
            value={service.url}
            size={208}
            level="M"
            marginSize={4}
            title={service.title + "微信二维码"}
            role="img"
            aria-label={service.title + "微信二维码"}
          />
          <figcaption>使用微信扫码打开{service.title}服务</figcaption>
        </figure>
      </Card.Content>
    </Card>
  );
}

export default function CampusPaymentsPage() {
  const { state } = useAuth();
  const navigate = useNavigate();
  const authenticated = "Authenticated" in state;
  const checking = "Checking" in state;
  const [busy, setBusy] = useState(false);
  const opening = useRef(false);
  const mounted = useRef(false);
  const [notice, setNotice] = useState<string>();
  const noticeRevision = useRef(0);
  // Reuse this one-shot read across StrictMode's effect replay. A discarded
  // first callback must not consume and lose the only failure notice.
  const noticeRequest = useRef<Promise<NetworkFeePortalNotice | null> | undefined>(undefined);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    if (!checking && !authenticated) void navigate({ to: "/", replace: true });
  }, [authenticated, checking, navigate]);

  useEffect(() => {
    if (!authenticated) return;
    let active = true;
    const revision = noticeRevision.current;
    noticeRequest.current ??= invoke<NetworkFeePortalNotice | null>(
      "consume_network_fee_portal_notice",
    );
    void noticeRequest.current
      .then((next) => {
        if (active && revision === noticeRevision.current && next) setNotice(next.message);
      })
      .catch(() => {
        if (active && revision === noticeRevision.current)
          setNotice("未能读取上次网费办理提示，可重新打开官网。");
      });
    return () => {
      active = false;
    };
  }, [authenticated]);

  const openNetworkFeePortal = async () => {
    if (!authenticated || opening.current) return;
    opening.current = true;
    setBusy(true);
    noticeRevision.current++;
    setNotice(undefined);
    try {
      // The destination and credentials are owned by Rust, never React.
      await invoke("open_network_fee_portal");
    } catch {
      if (mounted.current) {
        setNotice("无法打开网费官网，请检查网络或重新认证后重试。");
        setBusy(false);
        opening.current = false;
      }
    }
  };

  if (!authenticated) {
    return checking ? (
      <PageShell centered>
        <Spinner size="lg" aria-label="正在检查统一认证状态" />
      </PageShell>
    ) : null;
  }

  return (
    <PageShell>
      <div className="payments-layout">
        <CloseButton
          className="payments-back"
          aria-label="返回工作台"
          onPress={() => void navigate({ to: "/" })}
        >
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <header className="payments-header">
          <span className="eyebrow">CAMPUS SERVICES</span>
          <h1>校园卡/电费/网费</h1>
          <p className="page-description">
            仅提供官方服务入口与登录辅助；不查询余额或账单，不代办充值、扣款或套餐变更。
          </p>
        </header>
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
            <Button variant="primary" isDisabled={busy} onPress={() => void openNetworkFeePortal()}>
              {busy && <Spinner size="sm" color="current" />}
              {busy ? "正在打开网费官网…" : "打开网费官网"}
            </Button>
            <p className="payments-hint">
              每次打开最多自动提交一次登录；遇到验证码或登录错误将停止自动操作。办理时可随时返回本页。
            </p>
          </Card.Content>
        </Card>
        <div className="payments-wechat-grid">
          {WECHAT_SERVICES.map((service) => (
            <WeChatServiceCard key={service.id} service={service} />
          ))}
        </div>
      </div>
    </PageShell>
  );
}
