import { createContext, useCallback, useContext, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Dialog, Button } from "material/react";
import { toast } from "../ui";
import type { Probe } from "./model";

type Pending = { reason: string; resolve: (granted: boolean) => void };

type EnsureWebvpn = (reason: string) => Promise<boolean>;

const GateContext = createContext<EnsureWebvpn | null>(null);

/** 没有挂 Provider 时直接放行：需要 2FA 的场景由后端在真正操作时报错。 */
const allowWithoutGate: EnsureWebvpn = async () => true;

/**
 * WebVPN 认证闸门。
 *
 * WebVPN 不再有独立页面与入口按钮：需要校外访问的功能在发起操作前先探测，
 * 只有探测到「需要 2FA」时才弹出这个 Dialog 询问是否立即认证。
 *
 * Provider 只挂在选课模块内部，其它临时启用的功能若要使用同一个 Dialog，
 * 自行用 `<WebvpnAuthProvider>` 包一层即可；缺失时闸门退化为放行。
 */
export function useWebvpnGate(): EnsureWebvpn {
  return useContext(GateContext) ?? allowWithoutGate;
}

/** 探测当前 WebVPN 会话是否还需要 2FA。探测失败时不打扰用户，交给后端在操作时报错。 */
async function needsWebvpn2fa(): Promise<boolean> {
  try {
    const probes = await invoke<Probe[]>("enrollment_command", { action: { type: "probe" } });
    return probes?.some((probe) => probe.mode === "webvpn" && probe.needs2fa) ?? false;
  } catch {
    return false;
  }
}

export function WebvpnAuthProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<Pending>();

  const settle = useCallback((granted: boolean) => {
    setPending((current) => {
      current?.resolve(granted);
      return undefined;
    });
  }, []);

  const ensure = useCallback<EnsureWebvpn>(async (reason) => {
    if (!(await needsWebvpn2fa())) return true;
    const granted = await new Promise<boolean>((resolve) => setPending({ reason, resolve }));
    if (!granted) return false;
    try {
      await invoke("open_academic_portal", { kind: "webvpn", target: null });
      return true;
    } catch (cause) {
      toast.danger("无法打开 WebVPN 认证窗口", {
        description: typeof cause === "string" ? cause : "请稍后重试。",
      });
      return false;
    }
  }, []);

  return (
    <GateContext.Provider value={ensure}>
      {children}
      {pending &&
        createPortal(
          <Dialog
            open
            onClose={() => settle(false)}
            headline="需要 WebVPN 认证"
            subtitle="校外访问校园系统需要先在学校专属页面完成 2FA。"
            size="small"
            actions={
              <>
                <Button variant="text" onClick={() => settle(false)}>
                  稍后再说
                </Button>
                <Button variant="filled" onClick={() => settle(true)}>
                  打开认证窗口
                </Button>
              </>
            }
          >
            <p>即将{pending.reason}，需要先完成 WebVPN 专属 2FA 认证；认证成功后可继续当前操作。</p>
          </Dialog>,
          document.body,
        )}
    </GateContext.Provider>
  );
}
