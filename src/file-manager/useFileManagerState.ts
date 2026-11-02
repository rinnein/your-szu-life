import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { FILE_MANAGER_CHANGED_EVENT, getFileManagerState, type FileManagerState } from "./api";

/**
 * 统一文件目录状态。
 *
 * 目录是缓存型数据：挂载时取一次快照，之后靠后端 `file-manager-changed` 事件
 * 更新，不做轮询。
 */
export function useFileManagerState() {
  const [state, setState] = useState<FileManagerState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      setState(await getFileManagerState());
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<FileManagerState>(FILE_MANAGER_CHANGED_EVENT, (event) => {
      if (!disposed) {
        setState(event.payload);
        setError(null);
      }
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  return { state, error, loading, refresh };
}
