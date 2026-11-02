import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button, Dialog } from "material/react";
import { isChildDocument } from "./logical-navigation";

export interface PortNotice {
  id: number;
  kind: "mcp-port-changed" | "ports-exhausted";
  service: "mcp" | "aria2" | "jxxk-download";
  previousPort: number | null;
  currentPort: number | null;
  message: string;
}

const TITLES: Record<PortNotice["kind"], string> = {
  "mcp-port-changed": "MCP 端口已切换",
  "ports-exhausted": "端口分配失败",
};

/**
 * 本地服务端口通知的唯一出口。
 *
 * 后端在端口耗尽时记录日志并投递通知；MCP 端口被占用而切换时也会投递一条。
 * 启动阶段后端还没等到前端监听器，会把通知暂存到队列里，因此这里必须
 * **先注册事件监听器，再调用 `take_port_notices`**：后端在 take 时才把前端
 * 标记为就绪，这个顺序保证就绪之后发出的通知不会丢失。
 *
 * 与 NotificationCenter 一样只主文档运行，避免多个 WebView 重复弹窗。
 */
export function PortNoticeBridge() {
  const [notices, setNotices] = useState<PortNotice[]>([]);
  const seen = useRef(new Set<number>());
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || isChildDocument()) return;
    if (!("__TAURI_INTERNALS__" in window)) return;

    let cancelled = false;
    let stop: (() => void) | undefined;

    const enqueue = (incoming: PortNotice[]) => {
      if (!mounted.current || incoming.length === 0) return;
      setNotices((current) => {
        const next = [...current];
        for (const notice of incoming) {
          if (seen.current.has(notice.id)) continue;
          seen.current.add(notice.id);
          next.push(notice);
        }
        return next;
      });
    };

    void listen<PortNotice>("app://port-notice", (event) => {
      if (event.payload) enqueue([event.payload]);
    }).then((unlisten) => {
      if (cancelled) {
        unlisten();
        return;
      }
      stop = unlisten;
      void invoke<PortNotice[]>("take_port_notices")
        .then((pending) => enqueue(Array.isArray(pending) ? pending : []))
        .catch(() => undefined);
    });

    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  const notice = notices[0];
  if (!notice) return null;

  const dismiss = () => setNotices((current) => current.slice(1));

  return createPortal(
    <Dialog
      open
      onClose={dismiss}
      headline={TITLES[notice.kind] ?? "端口通知"}
      size="small"
      actions={
        <Button variant="filled" onClick={dismiss}>
          知道了
        </Button>
      }
    >
      <p className="text-sm leading-relaxed">{notice.message}</p>
    </Dialog>,
    document.body,
  );
}
