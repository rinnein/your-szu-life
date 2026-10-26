import { useEffect, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "material/react";
import type { Mode } from "./model";
type Batch = {
  code?: string;
  name?: string;
  typeName?: string;
  tacticName?: string;
  beginTime?: string;
  endTime?: string;
  active?: string | number;
};
type Info = {
  batches?: Batch[];
  online?: number;
  updatedAt?: number;
  error?: string;
  pending?: boolean;
};
const entries = new Map<string, Info>();
const inflight = new Map<string, Promise<void>>();
const listeners = new Set<() => void>();
const empty: Info = {};
const emit = () => listeners.forEach((fn) => fn());
async function fetchInfo(key: string, type: "batchInfo" | "onlineUsers", mode: Mode) {
  if (inflight.has(key)) return inflight.get(key);
  entries.set(key, { ...entries.get(key), pending: true });
  emit();
  const request = (async () => {
    try {
      const raw = await invoke<{
        dataList?: Batch[];
        data?: { onlineUsers?: number };
        onlineUsers?: number;
      }>("enrollment_command", { action: { type, mode } });
      const value =
        type === "batchInfo"
          ? { batches: raw.dataList }
          : { online: raw.data?.onlineUsers ?? raw.onlineUsers };
      if (
        type === "batchInfo"
          ? !Array.isArray(value.batches)
          : !Number.isFinite(Number(value.online)) || value.online == null
      )
        throw Error("学校信息格式暂不可用");
      entries.set(key, { ...value, updatedAt: Date.now() });
    } catch (e) {
      entries.set(key, { ...entries.get(key), error: String(e), pending: false });
    } finally {
      inflight.delete(key);
      emit();
    }
  })();
  inflight.set(key, request);
  return request;
}
const observers = new Map<string, { count: number; stop: () => void }>();
function observe(key: string, mode: Mode) {
  const old = observers.get(key);
  if (old) old.count++;
  else {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let live = true;
    let polling = false;
    const poll = async () => {
      if (!live || document.hidden || polling) return;
      polling = true;
      await fetchInfo(key, "onlineUsers", mode);
      polling = false;
      if (live && !document.hidden) timer = setTimeout(poll, 60000);
    };
    const visibility = () => {
      clearTimeout(timer);
      if (!document.hidden) void poll();
    };
    document.addEventListener("visibilitychange", visibility);
    void poll();
    observers.set(key, {
      count: 1,
      stop: () => {
        live = false;
        clearTimeout(timer);
        document.removeEventListener("visibilitychange", visibility);
      },
    });
  }
  return () => {
    const item = observers.get(key);
    if (item && --item.count === 0) {
      item.stop();
      observers.delete(key);
    }
  };
}
function useInfo(account: string, mode: Mode, kind: "batchInfo" | "onlineUsers") {
  const key = `${account}:${mode}:${kind}`;
  const value = useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    () => entries.get(key) ?? empty,
  );
  return { key, value };
}
export function OnlineUsers({ account, mode }: { account: string; mode: Mode }) {
  const { key, value } = useInfo(account, mode, "onlineUsers");
  useEffect(() => observe(key, mode), [key, mode]);
  return (
    <span className="text-xs text-muted">
      在线人数：{value.online ?? "暂不可用"}
      {value.updatedAt && ` · ${new Date(value.updatedAt).toLocaleTimeString()}`}
      {value.error && "（更新失败）"}
    </span>
  );
}
export function BatchInfo({ account, mode }: { account: string; mode: Mode }) {
  const { key, value } = useInfo(account, mode, "batchInfo");
  const [visible, setVisible] = useState(!document.hidden);
  useEffect(() => {
    const fn = () => setVisible(!document.hidden);
    document.addEventListener("visibilitychange", fn);
    return () => document.removeEventListener("visibilitychange", fn);
  }, []);
  useEffect(() => {
    if (visible) void fetchInfo(key, "batchInfo", mode);
  }, [key, mode, visible]);
  const rows = value.batches ?? [];
  return (
    <section aria-label="当前选课批次" className="grid gap-2 rounded-xl bg-default p-4 text-sm">
      <div className="flex items-center justify-between">
        <strong>当前选课批次</strong>
        <Button
          size="xs"
          variant="text"
          disabled={!!value.pending}
          onClick={() => void fetchInfo(key, "batchInfo", mode)}
        >
          刷新批次
        </Button>
      </div>
      {!rows.some((row) => String(row.active) === "1") && <p>学校未标记当前激活批次</p>}
      {[...rows]
        .sort((a, b) => Number(String(b.active) === "1") - Number(String(a.active) === "1"))
        .map((row, i) => (
          <div key={row.code ?? i}>
            <p>
              {row.name ?? "未命名批次"} · {row.typeName ?? "阶段未提供"}
            </p>
            <p>策略：{row.tacticName ?? "未提供"}</p>
            <p>
              {row.beginTime ?? "开始时间未提供"} — {row.endTime ?? "结束时间未提供"}
            </p>
          </div>
        ))}
      {value.error && (
        <p role="status">
          {value.error}
          {value.updatedAt ? "（显示历史数据）" : ""}
        </p>
      )}
      {value.updatedAt && (
        <p className="text-xs text-muted">更新于 {new Date(value.updatedAt).toLocaleString()}</p>
      )}
    </section>
  );
}
