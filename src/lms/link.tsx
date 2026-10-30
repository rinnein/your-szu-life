import { useState } from "react";
import { Button, Dialog } from "material/react";
import { toast } from "../ui";
import { copyText } from "../clipboard";
import { openPublicUrl } from "../app-content/external";
import { isAndroidPlatform } from "../os-info";
import type { LmsTodo } from "./types";

/** 根据待办的 course_id 与任务 id 构造畅课任务页面链接。 */
export function lmsTodoUrl(todo: Pick<LmsTodo, "course_id" | "id">): string {
  return `https://lms.szu.edu.cn/course/${todo.course_id}/learning-activity#/${todo.id}?view=scores`;
}

/** 打开外部链接：失败时用 toast 反馈，不向上抛错。 */
export async function openLmsExternalUrl(url: string): Promise<void> {
  try {
    await openPublicUrl(url);
  } catch (error) {
    toast.danger("无法打开系统浏览器", { description: String(error) });
  }
}

export interface LmsTodoLink {
  title: string;
  url: string;
}

/** 平台相关的待办链接打开行为：桌面直接开浏览器，Android 弹链接模态框。 */
export async function openLmsTodoLink(
  todo: Pick<LmsTodo, "course_id" | "id" | "title">,
  showDialog: (link: LmsTodoLink) => void,
): Promise<void> {
  const link: LmsTodoLink = { title: todo.title, url: lmsTodoUrl(todo) };
  if (await isAndroidPlatform()) {
    showDialog(link);
    return;
  }
  await openLmsExternalUrl(link.url);
}

/** Android 端的待办链接模态框：显示链接，提供复制与浏览器打开两个按钮。 */
export function LmsTodoLinkDialog({
  link,
  onClose,
}: {
  link: LmsTodoLink | null;
  onClose: () => void;
}) {
  const [busy, setBusy] = useState(false);
  if (!link) return null;

  const handleCopy = async () => {
    const ok = await copyText(link.url);
    if (ok) {
      toast.success("任务链接已复制到剪贴板");
    } else {
      toast.danger("复制失败，请长按链接手动复制");
    }
  };

  const handleOpen = async () => {
    setBusy(true);
    try {
      await openLmsExternalUrl(link.url);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      headline="打开畅课任务"
      subtitle={link.title}
      size="small"
      actions={
        <>
          <Button variant="text" onClick={onClose}>
            关闭
          </Button>
          <Button variant="outlined" onClick={() => void handleCopy()}>
            复制链接
          </Button>
          <Button variant="filled" disabled={busy} onClick={() => void handleOpen()}>
            在浏览器打开
          </Button>
        </>
      }
    >
      <code className="block break-all rounded bg-surface-container px-3 py-2 text-xs text-muted select-all">
        {link.url}
      </code>
    </Dialog>
  );
}
