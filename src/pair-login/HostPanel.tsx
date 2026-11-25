import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, LoadingIndicator } from "material/react";
import { QRCodeSVG } from "qrcode.react";
import { Banner, toast } from "../ui";
import { errorMessage } from "../async-state";
import { usePending } from "../use-pending";
import { copyText } from "../clipboard";
import { pairLoginHostCancel, pairLoginHostStart, pairLoginTakeCredentials } from "./api";
import type { PairCredentials, PairHostView } from "./types";

/** 事件名与 Rust 侧 `PAIR_HOST_EVENT` 一致。 */
const HOST_EVENT = "pair-login-host-changed";

/**
 * 未登录设备的配对面板。
 *
 * 只在用户点击「远程授权登录」后才挂载，因此这里不做挂载即启动；也不在卸载时
 * 取消会话——React StrictMode 会模拟一次卸载/重挂，会把刚创建的房间立刻销毁。
 * 取消由用户显式按钮触发，其它情况由 host 端的 10 分钟过期兜底。
 */
export function PairHostPanel({
  initial,
  onCancel,
  onAuthorized,
}: {
  initial: PairHostView;
  onCancel: () => void;
  onAuthorized: (credentials: PairCredentials) => void;
}) {
  const [view, setView] = useState(initial);
  const pending = usePending();
  const startedTake = useRef(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<PairHostView>(HOST_EVENT, ({ payload }) => {
      if (!active) return;
      setView(payload);
      // 过期后节点已由 Rust 侧停止；顺手清掉会话，释放回环监听端口。
      if (payload.status === "expired") {
        void pairLoginHostCancel().catch(() => undefined);
      }
    })
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  // 收到授权后只取一次；take 成功后 Rust 侧会话即被销毁。
  useEffect(() => {
    if (view.status !== "accepted" || startedTake.current) return;
    startedTake.current = true;
    void pairLoginTakeCredentials()
      .then((credentials) => {
        if (credentials) onAuthorized(credentials);
        else toast.warning("授权凭据已被使用，请重新发起配对");
      })
      .catch((error) => {
        startedTake.current = false;
        toast.danger("读取授权结果失败", { description: errorMessage(error) });
      });
  }, [view.status, onAuthorized]);

  const cancel = useCallback(async () => {
    await pairLoginHostCancel().catch(() => undefined);
    onCancel();
  }, [onCancel]);

  const restart = async () => {
    const result = await pending.run("restart", () => pairLoginHostStart());
    if (!result.ok) {
      if (!result.skipped) {
        toast.danger("重新生成配对失败", { description: errorMessage(result.error) });
      }
      return;
    }
    startedTake.current = false;
    setCopied(false);
    setView(result.value);
  };

  const copyLink = async () => {
    const ok = await copyText(view.deepLink);
    setCopied(ok);
    if (ok) toast.success("配对链接已复制");
    else toast.warning("无法自动复制，请手动选择下方链接");
  };

  if (view.status === "expired") {
    return (
      <Card variant="outlined">
        <div slot="headline">远程授权登录</div>
        <div slot="subhead">配对码 10 分钟内有效，过期后需要重新生成。</div>
        <div className="grid justify-items-start gap-3">
          <Banner status="warning">
            <Banner.Title>配对已过期</Banner.Title>
            <Banner.Description>
              新设备上的配对码已失效。重新生成后，请在已登录设备上重新扫码。
            </Banner.Description>
          </Banner>
          <Button
            variant="filled"
            disabled={pending.isPending("restart")}
            onClick={() => void restart()}
          >
            {pending.isPending("restart") && <LoadingIndicator size={16} />}
            重新生成
          </Button>
          <Button variant="text" onClick={() => void cancel()}>
            取消，改为手动填写
          </Button>
        </div>
      </Card>
    );
  }

  if (view.status === "accepted") {
    return (
      <Card variant="outlined">
        <div slot="headline">远程授权登录</div>
        <div slot="subhead">已收到授权凭据，正在用现有流程完成统一认证。</div>
        <p className="status-line" role="status">
          <LoadingIndicator size={20} aria-hidden="true" />
          正在登录…
        </p>
      </Card>
    );
  }

  return (
    <Card variant="outlined">
      <div slot="headline">远程授权登录</div>
      <div slot="subhead">
        在已登录的设备上扫描二维码，核对两台设备上的 6 位配对码一致后确认授权。
      </div>
      <div className="grid justify-items-center gap-3">
        <QRCodeSVG
          value={view.deepLink}
          size={208}
          level="M"
          marginSize={4}
          title="远程授权登录二维码"
          role="img"
          aria-label="远程授权登录二维码"
        />
        <p
          className="text-3xl font-semibold tracking-[0.3em] tabular-nums"
          aria-label={`配对码 ${view.code}`}
        >
          {view.code}
        </p>
        <p className="text-xs text-muted">请确认已登录设备上显示同样的 6 位配对码。</p>
        <code className="max-w-full break-all text-xs text-muted" tabIndex={0}>
          {view.deepLink}
        </code>
        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button variant="tonal" onClick={() => void copyLink()}>
            {copied ? "已复制" : "复制链接"}
          </Button>
          <Button variant="text" onClick={() => void cancel()}>
            取消，改为手动填写
          </Button>
        </div>
        {view.status === "connected" ? (
          <Banner status="info">
            <Banner.Title>已连接，等待确认登录</Banner.Title>
            <Banner.Description>
              已连接到一台已登录设备。请在那边核对配对码并点击「确认授权」。
            </Banner.Description>
          </Banner>
        ) : (
          <p className="status-line" role="status">
            <LoadingIndicator size={20} aria-hidden="true" />
            等待已登录设备扫码…
          </p>
        )}
      </div>
    </Card>
  );
}
