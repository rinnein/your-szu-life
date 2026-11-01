import { useCallback, useState } from "react";
import { useChannelSubscription } from "../tauri-channel";
import { pauseDownload, removeDownload, resumeDownload, type JxxkDownloadTask } from "./api";

/**
 * 巡课录像下载任务列表：Tauri Channel 订阅（ws/sse 形式的服务端推送）——
 * 后端任务每次变化主动推送全量快照，无任何前端轮询。
 * WatchPage 的内联进度与下载管理页共用同一份数据源。
 */
export function useJxxkDownloads() {
  const [tasks, setTasks] = useState<JxxkDownloadTask[]>([]);

  // 后端订阅命令会先推送当前快照，此后每次变化自动推送。
  useChannelSubscription<JxxkDownloadTask[]>("subscribe_jxxk_downloads", (list) =>
    setTasks(list ?? []),
  );

  const pause = useCallback(async (taskId: string) => {
    await pauseDownload(taskId);
  }, []);

  const resume = useCallback(async (taskId: string) => {
    await resumeDownload(taskId);
  }, []);

  const remove = useCallback(async (taskId: string, deleteFile: boolean) => {
    await removeDownload(taskId, deleteFile);
  }, []);

  return { tasks, pause, resume, remove };
}
