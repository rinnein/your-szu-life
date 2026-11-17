/**
 * 签到列表：单个描边容器内以分隔线分行，替代逐条叠放的 outlined 卡片。
 *
 * 可点击行用原生 button 而不是 material List——List 的点击语义是「选中行」，
 * 这里是「打开对话框」（参照 src/timetable/SessionCard.tsx 的先例）。
 */
import type { ReactNode } from "react";

export function SigninList({ label, children }: { label: string; children: ReactNode }) {
  return (
    <ul
      aria-label={label}
      className="m-0 list-none overflow-hidden rounded-xl border border-separator p-0"
    >
      {children}
    </ul>
  );
}

export function SigninRow({
  title,
  supporting,
  trailing,
  onClick,
}: {
  title: ReactNode;
  supporting?: ReactNode;
  trailing?: ReactNode;
  onClick?: () => void;
}) {
  const body = (
    <>
      <span className="min-w-0 flex-1">
        <span className="block font-medium leading-snug break-words">{title}</span>
        {supporting && (
          <span className="mt-0.5 block text-sm text-muted tabular-nums">{supporting}</span>
        )}
      </span>
      {trailing && <span className="flex shrink-0 items-center gap-2">{trailing}</span>}
    </>
  );
  return (
    <li className="border-separator [&:not(:first-child)]:border-t">
      {onClick ? (
        <button
          type="button"
          className="flex min-h-16 w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-surface"
          onClick={onClick}
        >
          {body}
        </button>
      ) : (
        <div className="flex min-h-16 items-center gap-3 px-4 py-3">{body}</div>
      )}
    </li>
  );
}
