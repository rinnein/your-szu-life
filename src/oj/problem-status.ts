import { verdicts } from "./judging";
import type { Problem } from "./types";

/** 与 material 的语义色同名，判定色因此能直接落到 MD3 令牌上。 */
export type StatusTone = "success" | "danger" | "warning" | "accent" | "default";

/** 胶囊底色与文字色，两种主题都由令牌决定。 */
const STATUS_PILL_COLORS: Record<StatusTone, [background: string, foreground: string]> = {
  success: ["var(--mtrl-sys-color-success)", "var(--mtrl-sys-color-on-success)"],
  danger: ["var(--mtrl-sys-color-error)", "var(--mtrl-sys-color-on-error)"],
  warning: ["var(--mtrl-sys-color-warning)", "var(--mtrl-sys-color-on-warning)"],
  accent: ["var(--mtrl-sys-color-primary)", "var(--mtrl-sys-color-on-primary)"],
  default: [
    "color-mix(in srgb, var(--mtrl-sys-color-on-surface-variant) 14%, transparent)",
    "var(--mtrl-sys-color-on-surface-variant)",
  ],
};

/**
 * 一道题当前的判定色与短文字。
 *
 * 有服务端判定时用判定码（AC/WA/TLE…），否则退回本地的作答状态（已作答/未提交…）：
 * 非编程题只有本地草稿、拿不到服务端判定，本地状态就是它唯一的信号。
 */
export function problemStatus(
  problem: Problem,
  verdict: (problem: Problem) => number | undefined,
  emptyVerdict: (problem: Problem) => string,
): { tone: StatusTone; text: string } {
  const status = verdict(problem);
  if (status != null) {
    const judged = verdicts[status];
    return { tone: judged?.color ?? "default", text: judged?.code ?? `状态 ${status}` };
  }
  const empty = emptyVerdict(problem);
  return { tone: empty === "已作答" ? "success" : "default", text: empty };
}

/**
 * 判定胶囊的 HTML 片段。
 *
 * material 的 `<m-drawer-item>` 与 `<m-tab>` 只接受字符串形式的 `icon`，并把它
 * `innerHTML` 进自己的 shadow root。shadow 内的元素不受文档级 CSS 影响，因此样式
 * 只能写在行内；颜色仍取 MD3 令牌，令牌会从宿主继承进 shadow tree，深浅主题都跟随。
 *
 * `text` 来自判定表与本地的作答状态文案，不含用户输入。
 */
export function statusPillHtml(tone: StatusTone, text: string): string {
  const [background, foreground] = STATUS_PILL_COLORS[tone];
  return (
    '<span style="display:inline-flex;align-items:center;justify-content:center;' +
    "min-inline-size:36px;block-size:20px;padding-inline:6px;border-radius:9999px;" +
    `font-size:11px;font-weight:600;letter-spacing:0.02em;background:${background};color:${foreground}">` +
    `${text}</span>`
  );
}
