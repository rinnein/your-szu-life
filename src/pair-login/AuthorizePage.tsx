import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "@tanstack/react-router";
import { Button, Card, LoadingIndicator } from "material/react";
import { Banner, toast } from "../ui";
import { errorMessage } from "../async-state";
import { usePending } from "../use-pending";
import { PageSurface } from "../shell/PageSurface";
import { ModuleHeader } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { usePageBackHandler } from "../page-back";
import { platformLabel } from "../platform-label";
import { pairLoginAuthorize, pairLoginCancel, pairLoginConnect } from "./api";
import type { PairPeerView } from "./types";

type PageState =
  | { kind: "connecting" }
  | { kind: "connected"; peer: PairPeerView }
  | { kind: "sending"; peer: PairPeerView }
  | { kind: "sent"; peer: PairPeerView }
  | { kind: "failed"; message: string; peer: PairPeerView | null };

/** 已登录设备上的授权页：连接新设备、核对 6 位码、确认后发送 vault 凭据。 */
export function PairAuthorizePage() {
  const navigation = useLogicalNavigation();
  const router = useRouter();
  const pending = usePending();
  const [state, setState] = useState<PageState>({ kind: "connecting" });
  const started = useRef(false);

  const connect = useCallback(async () => {
    setState({ kind: "connecting" });
    try {
      const peer = await pairLoginConnect();
      setState({ kind: "connected", peer });
    } catch (error) {
      setState({
        kind: "failed",
        message: errorMessage(error, "无法连接新设备"),
        peer: null,
      });
    }
  }, []);

  // StrictMode 会重放 effect；只允许发起一次连接。
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void connect();
  }, [connect]);

  // 明确离开页面时停掉配对房间；路由切换与子窗口关闭都走这里。
  const cancelQuietly = useCallback(() => pairLoginCancel().catch(() => undefined), []);

  const leave = useCallback(async () => {
    await cancelQuietly();
    await navigation.back();
  }, [cancelQuietly, navigation]);

  const systemBack = useCallback(async () => {
    await cancelQuietly();
    router.history.back();
  }, [cancelQuietly, router]);
  usePageBackHandler(systemBack);

  // 子窗口被系统关闭时没有显式返回入口，用 pagehide 做一次尽力清理；
  // 即使 IPC 没送达，Rust 侧邀请有效期也会停止房间。
  useEffect(() => {
    const onPageHide = () => {
      void pairLoginCancel().catch(() => undefined);
    };
    window.addEventListener("pagehide", onPageHide);
    return () => window.removeEventListener("pagehide", onPageHide);
  }, []);

  const authorize = async (peer: PairPeerView) => {
    setState({ kind: "sending", peer });
    const result = await pending.run("authorize", () => pairLoginAuthorize());
    if (!result.ok) {
      if (result.skipped) return;
      setState({ kind: "failed", message: errorMessage(result.error), peer });
      return;
    }
    setState({ kind: "sent", peer });
    toast.success("已授权，新设备正在登录");
  };

  return (
    <PageSurface maxWidth="md">
      <ModuleHeader
        title="授权登录"
        description="为一台正在初始化的新设备完成统一认证登录。"
        onBack={() => void leave()}
      />
      <AuthorizeBody
        state={state}
        busy={pending.isPending("authorize")}
        onRetryConnect={() => void connect()}
        onRetryAuthorize={(peer) => void authorize(peer)}
        onAuthorize={(peer) => void authorize(peer)}
        onLeave={() => void leave()}
      />
    </PageSurface>
  );
}

function AuthorizeBody({
  state,
  busy,
  onRetryConnect,
  onRetryAuthorize,
  onAuthorize,
  onLeave,
}: {
  state: PageState;
  busy: boolean;
  onRetryConnect: () => void;
  onRetryAuthorize: (peer: PairPeerView) => void;
  onAuthorize: (peer: PairPeerView) => void;
  onLeave: () => void;
}) {
  switch (state.kind) {
    case "connecting":
      return (
        <Card variant="outlined">
          <div slot="headline">正在连接新设备</div>
          <div slot="subhead">会通过临时配对网络查找刚才显示配对码的设备。</div>
          <p className="status-line" role="status">
            <LoadingIndicator size={20} aria-hidden="true" />
            正在连接，最多等待 30 秒…
          </p>
        </Card>
      );

    case "connected":
    case "sending":
      return (
        <Card variant="outlined">
          <div slot="headline">确认授权登录</div>
          <div slot="subhead">
            对方设备将使用本机保存的统一认证账户完成登录。请先核对设备与配对码。
          </div>
          <dl className="detail-list">
            <dt>设备</dt>
            <dd>{state.peer.device.name || "未命名设备"}</dd>
            <dt>系统</dt>
            <dd>
              {platformLabel(state.peer.device.platform)}
              {state.peer.device.appVersion ? ` · ${state.peer.device.appVersion}` : ""}
            </dd>
            <dt>配对码</dt>
            <dd>
              <span className="text-xl font-semibold tracking-[0.2em] tabular-nums">
                {state.peer.code}
              </span>
            </dd>
          </dl>
          <Banner status="warning">
            <Banner.Title>只在你面前的新设备上确认</Banner.Title>
            <Banner.Description>
              确认后会把本机保存的校园卡号和统一认证密码发送给该设备。若设备名称或 6
              位配对码与新设备屏幕上的不一致，请取消。
            </Banner.Description>
          </Banner>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              variant="filled"
              disabled={state.kind === "sending" || busy}
              onClick={() => onAuthorize(state.peer)}
            >
              {state.kind === "sending" && <LoadingIndicator size={16} />}
              确认授权
            </Button>
            <Button variant="text" disabled={state.kind === "sending"} onClick={onLeave}>
              取消
            </Button>
          </div>
        </Card>
      );

    case "sent":
      return (
        <Card variant="outlined">
          <div slot="headline">已授权</div>
          <div slot="subhead">
            授权凭据已经发送给 {state.peer.device.name || "新设备"}，那边会自动继续统一认证。
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="filled" onClick={onLeave}>
              完成
            </Button>
          </div>
        </Card>
      );

    case "failed": {
      const failedPeer = state.peer;
      return (
        <>
          <Banner status="danger">
            <Banner.Title>授权未完成</Banner.Title>
            <Banner.Description>{state.message}</Banner.Description>
          </Banner>
          <div className="mt-3 flex flex-wrap gap-2">
            {failedPeer ? (
              <Button variant="filled" disabled={busy} onClick={() => onRetryAuthorize(failedPeer)}>
                重新发送
              </Button>
            ) : (
              <Button variant="filled" onClick={onRetryConnect}>
                重试连接
              </Button>
            )}
            <Button variant="text" onClick={onLeave}>
              取消
            </Button>
          </div>
        </>
      );
    }
  }
}
