import { io } from "socket.io-client";
export interface CphStatus {
  connected: boolean;
  active: boolean;
  message: string;
}
export interface CphSubmission {
  url: string;
  sourceCode: string;
}
export function validCphSubmission(value: unknown): value is CphSubmission {
  return (
    !!value &&
    typeof value === "object" &&
    typeof (value as CphSubmission).url === "string" &&
    typeof (value as CphSubmission).sourceCode === "string" &&
    (value as CphSubmission).url.length <= 4096 &&
    (value as CphSubmission).sourceCode.length <= 512 * 1024
  );
}
export function connectCph(
  port: number,
  changed: (status: CphStatus) => void,
  submit: (request: CphSubmission) => Promise<unknown>,
  factory = io,
) {
  const socket = factory(`http://127.0.0.1:${port}`, {
    forceNew: true,
    path: "/ws",
    transports: ["websocket"],
    query: { type: "browser", clientId: crypto.randomUUID() },
    reconnectionDelay: 1000,
    reconnectionDelayMax: 10000,
    timeout: 3000,
  });
  let disposed = false;
  let active = false;
  let busy = false;
  const status = (message: string) => {
    if (!disposed)
      changed({ connected: socket.connected, active: active && socket.connected, message });
  };
  socket.on("connect", () => {
    active = false;
    status("已连接，等待提交端状态");
  });
  socket.on("status", (value: { isActive?: boolean }) => {
    active = value?.isActive === true;
    status(active ? "泥深已作为 cph-ng 提交端" : "已连接，当前由其他浏览器处理提交");
  });
  socket.on("disconnect", () => {
    active = false;
    status("cph-ng 未连接，等待自动重连");
  });
  socket.on("connect_error", () => {
    active = false;
    status("无法连接 cph-ng，请检查 VS Code 与端口");
  });
  socket.on("submitRequest", async (value: unknown) => {
    if (disposed || !active) return;
    if (!validCphSubmission(value)) {
      status("收到无效的 cph-ng 提交请求");
      return;
    }
    if (busy) {
      status("上一份 cph-ng 提交仍在处理，请稍后重试");
      return;
    }
    busy = true;
    status("正在使用本实例保存的语言和编译选项提交…");
    try {
      await submit({ url: value.url, sourceCode: value.sourceCode });
      status("代码已提交，可在 OJ 提交记录中查看结果");
    } catch (error) {
      status(String(error));
    } finally {
      busy = false;
    }
  });
  return {
    activate: () => {
      if (!disposed && socket.connected) socket.emit("setActive");
    },
    disconnect: () => {
      disposed = true;
      socket.removeAllListeners();
      socket.disconnect();
    },
  };
}
