import { useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Button, Card, Checkbox, Dialog } from "material/react";
import { LuPause, LuPlay, LuTrash2 } from "react-icons/lu";
import { DownloadSession } from "../DownloadSession";
import { Chip, toast } from "../ui";
import { PageSurface } from "../shell/PageSurface";
import { TopAppBar } from "../shell/TopAppBar";
import { AuthGate } from "../shell/AuthGate";
import { useJxxkDownloads } from "./useDownloads";
import type { JxxkDownloadTask, JxxkDownloadTaskStatus } from "./api";

const STATUS_META: Record<
  JxxkDownloadTaskStatus,
  { label: string; color: "danger" | "success" | "default" | "warning" }
> = {
  preparing: { label: "准备中", color: "warning" },
  downloading: { label: "下载中", color: "danger" },
  saving: { label: "保存中", color: "warning" },
  paused: { label: "已暂停", color: "warning" },
  completed: { label: "已完成", color: "success" },
  failed: { label: "失败", color: "danger" },
  cancelled: { label: "已取消", color: "default" },
};

/** 下载管理列表主体：独立路由页与播放页覆盖层共用。 */
export function DownloadManagerView() {
  const { tasks, pause, resume, remove } = useJxxkDownloads();
  const [confirmRemove, setConfirmRemove] = useState<JxxkDownloadTask | null>(null);
  const [alsoDeleteFile, setAlsoDeleteFile] = useState(true);

  const groups = useMemo(() => {
    // 同一选集（课程+课次）的多角度任务放在同一分组内。
    const map = new Map<string, JxxkDownloadTask[]>();
    for (const task of tasks) {
      const key = `${task.courseName}|${task.lessonName}`;
      const list = map.get(key) ?? [];
      list.push(task);
      map.set(key, list);
    }
    return [...map.entries()]
      .map(([key, list]) => ({
        key,
        courseName: list[0].courseName,
        lessonName: list[0].lessonName,
        tasks: list.sort(
          (a, b) =>
            statusOrder(a.status) - statusOrder(b.status) ||
            a.streamName.localeCompare(b.streamName),
        ),
      }))
      .sort((a, b) => {
        const order = (g: { tasks: JxxkDownloadTask[] }) =>
          Math.min(...g.tasks.map((t) => statusOrder(t.status)));
        return order(a) - order(b) || b.tasks[0].createdAt - a.tasks[0].createdAt;
      });
  }, [tasks]);

  const doRemove = async (task: JxxkDownloadTask, withFile: boolean) => {
    try {
      await remove(task.id, withFile);
      toast.success("任务已删除", { description: task.fileName });
    } catch (error) {
      toast.danger("删除任务失败", { description: String(error) });
    }
  };

  return (
    <div className="flex flex-col gap-6">
      {groups.length === 0 ? (
        <div className="py-8 text-center text-sm text-muted">
          当前没有下载任务。在课次播放页选择画面即可开始下载。
        </div>
      ) : (
        groups.map((group) => (
          <section key={group.key} aria-label={group.lessonName}>
            <div className="flex items-center gap-2 mb-2">
              <h2 className="text-base font-medium text-foreground truncate">{group.lessonName}</h2>
              <span className="text-xs text-muted truncate">{group.courseName}</span>
            </div>
            <div className="flex flex-col gap-2.5">
              {group.tasks.map((task) => {
                const meta = STATUS_META[task.status];
                const active =
                  task.status === "downloading" ||
                  task.status === "preparing" ||
                  task.status === "saving";
                return (
                  <Card key={task.id} variant="outlined" className="p-3">
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 mb-1 flex-wrap">
                          <Chip size="sm" color={meta.color} variant="soft">
                            {meta.label}
                          </Chip>
                          <span className="text-xs text-muted truncate">{task.streamName}</span>
                        </div>
                        <div className="text-xs text-muted mt-1 truncate">{task.fileName}</div>
                        {task.error && (
                          <div className="text-xs text-danger mt-1 break-all">{task.error}</div>
                        )}
                      </div>
                      <div className="flex items-center gap-1 shrink-0">
                        {(task.status === "downloading" || task.status === "preparing") && (
                          <Button
                            variant="text"
                            onClick={() => void pause(task.id)}
                            aria-label={`暂停 ${task.lessonName} ${task.streamName}`}
                          >
                            <LuPause aria-hidden="true" />
                          </Button>
                        )}
                        {(task.status === "paused" ||
                          task.status === "failed" ||
                          task.status === "cancelled") && (
                          <Button
                            variant="text"
                            onClick={() => void resume(task.id)}
                            aria-label={`继续 ${task.lessonName} ${task.streamName}`}
                          >
                            <LuPlay aria-hidden="true" />
                          </Button>
                        )}
                        <Button
                          variant="text"
                          className="button-danger"
                          onClick={() => {
                            // 进行中的半截文件默认一并清理；已完成默认保留。
                            setAlsoDeleteFile(task.status !== "completed");
                            setConfirmRemove(task);
                          }}
                          aria-label={`删除 ${task.lessonName} ${task.streamName}`}
                        >
                          <LuTrash2 aria-hidden="true" />
                        </Button>
                      </div>
                    </div>
                    {/* 复用 aria2 下载卡片：展示分片位图、速率与连接数 */}
                    {(active || task.status === "paused") && (
                      <div className="mt-2">
                        <DownloadSession
                          title={task.streamName}
                          session={{
                            downloadedBytes: task.downloaded,
                            totalBytes: task.total,
                            speedBytesPerSec: task.speed,
                            port: task.port,
                            totalActiveTasks: task.totalActiveTasks,
                            connections: task.connections,
                            status: task.status,
                            numPieces: task.numPieces,
                            pieceLength: task.pieceLength,
                            bitfield: task.bitfield,
                            isResumed: task.isResumed,
                            resumedBytes: task.resumedBytes,
                            savingSince: task.savingSince,
                          }}
                          defaultExpanded={false}
                        />
                      </div>
                    )}
                    {task.status === "completed" && (
                      <div className="text-xs text-muted mt-1 break-all">
                        已保存到 {task.destDir}
                      </div>
                    )}
                  </Card>
                );
              })}
            </div>
          </section>
        ))
      )}

      {confirmRemove &&
        createPortal(
          <Dialog
            open
            onClose={() => setConfirmRemove(null)}
            headline="删除下载任务"
            size="small"
            actions={
              <>
                <Button variant="text" onClick={() => setConfirmRemove(null)}>
                  取消
                </Button>
                <Button
                  variant="filled"
                  className="button-danger"
                  onClick={() => {
                    const task = confirmRemove;
                    const withFile = alsoDeleteFile;
                    setConfirmRemove(null);
                    void doRemove(task, withFile);
                  }}
                >
                  删除
                </Button>
              </>
            }
          >
            <p>
              将删除「{confirmRemove.lessonName} · {confirmRemove.streamName}」的下载任务。
            </p>
            <label className="flex items-center gap-2 mt-2 cursor-pointer select-none">
              <Checkbox
                checked={alsoDeleteFile}
                onChange={(e: CustomEvent<{ checked: boolean }>) => {
                  setAlsoDeleteFile(e.detail?.checked ?? (e.target as HTMLInputElement).checked);
                }}
              />
              <span className="text-sm text-foreground">同时删除已下载的文件</span>
            </label>
          </Dialog>,
          document.body,
        )}
    </div>
  );
}

/** 录像下载管理：查看进度、暂停/继续与删除任务。 */
export default function DownloadManagerPage() {
  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar title="录像下载管理" description="巡课录像下载任务：进度、暂停与删除" />
        <DownloadManagerView />
      </PageSurface>
    </AuthGate>
  );
}

function statusOrder(status: JxxkDownloadTaskStatus): number {
  switch (status) {
    case "downloading":
    case "preparing":
    case "saving":
      return 0;
    case "paused":
      return 1;
    case "failed":
      return 2;
    case "completed":
      return 3;
    default:
      return 4;
  }
}
