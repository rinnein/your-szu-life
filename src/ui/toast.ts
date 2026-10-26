import "material/elements/css/snackbar";
import { defineSnackbar, type SnackbarElement } from "material/elements";

/**
 * 命令式提示桥接：把 `toast.success(...)` 落到 M3 的 `<m-snackbar>` 上。
 *
 * material 只提供 Snackbar 组件本身，没有命令式 API；这里负责创建元素、
 * 按类型给出时长与播报语义，并维护一份活动列表供测试清理。排队、超时、
 * 动作按钮与关闭逻辑都由 Snackbar 自己处理。
 */

export type ToastColor = "success" | "danger" | "warning" | "info" | "default";

export interface ToastOptions {
  /** 附加说明，以「·」接在主消息后。 */
  description?: string;
  /** 毫秒；`0` 表示直到手动关闭。默认按类型取值。 */
  timeout?: number;
  actionLabel?: string;
  onAction?: () => void;
  /** 消息关闭时的回调。 */
  onClose?: () => void;
}

/** 成功与普通提示 4 秒，警告 6 秒，错误常驻直到用户处理。 */
const DEFAULT_TIMEOUT: Record<ToastColor, number> = {
  success: 4000,
  info: 4000,
  warning: 6000,
  danger: 0,
  default: 4000,
};

class ToastQueue {
  /** 仍在显示或排队中的 snackbar，关闭时移除。 */
  private live = new Set<SnackbarElement>();

  show(color: ToastColor, message: string, options: ToastOptions = {}): string {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    if (typeof document === "undefined") return id;

    const timeout = options.timeout ?? DEFAULT_TIMEOUT[color];
    const text = options.description ? `${message} · ${options.description}` : message;
    const indefinite = timeout === 0;

    const host = document.createElement("m-snackbar") as SnackbarElement;
    host.setAttribute("message", text);
    host.setAttribute("duration", indefinite ? "indefinite" : String(timeout));
    host.setAttribute("position", "center");
    host.setAttribute("close-label", "关闭");
    if (indefinite) host.setAttribute("dismissible", "");
    if (options.actionLabel) host.setAttribute("action", options.actionLabel);

    const finish = () => {
      if (!this.live.delete(host)) return;
      host.remove();
      options.onClose?.();
    };
    host.addEventListener("close", finish, { once: true });
    if (options.actionLabel && options.onAction) {
      host.addEventListener("action", () => options.onAction?.(), { once: true });
    }

    document.body.appendChild(host);
    this.live.add(host);
    host.show();
    // 正文表面在 show() 之后才进入 top layer，因此设置在显示之后。
    this.announce(host, color);
    return id;
  }

  /**
   * Snackbar 的正文默认用 `role="status"`。错误改用 `alert` 让读屏立即打断，
   * 其余改成等价的 `aria-live="polite"`：不常驻 `status`，否则「页面上不应出现
   * 行内提示」这类断言会把瞬时提示误判为行内提示。
   */
  private announce(host: SnackbarElement, color: ToastColor): void {
    const surface = host.component?.element;
    if (!surface) return;
    if (color === "danger") {
      surface.setAttribute("role", "alert");
      surface.removeAttribute("aria-live");
    } else {
      surface.removeAttribute("role");
      surface.setAttribute("aria-live", "polite");
    }
  }

  /** 关闭当前并清空排队中的提示。 */
  clear(): void {
    for (const host of [...this.live]) {
      this.live.delete(host);
      try {
        host.hide();
      } catch {
        // 组件可能已被宿主移除，忽略。
      }
      host.remove();
    }
  }

  /** 清空提示并移除残留节点，供测试用例之间清理。 */
  reset(): void {
    this.clear();
    if (typeof document === "undefined") return;
    for (const host of document.querySelectorAll("m-snackbar")) host.remove();
  }
}

if (typeof customElements !== "undefined") defineSnackbar();

const queue = new ToastQueue();

function make(color: ToastColor) {
  return (message: string, options?: ToastOptions): string => queue.show(color, message, options);
}

/** 清空并移除所有提示；测试环境在每个用例后调用。 */
export function resetToasts(): void {
  queue.reset();
}

export const toast = {
  success: make("success"),
  danger: make("danger"),
  warning: make("warning"),
  info: make("info"),
  default: make("default"),
  clear: () => void queue.clear(),
};

export type Toast = typeof toast;
