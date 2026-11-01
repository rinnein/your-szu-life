import type Artplayer from "artplayer";

/**
 * ArtPlayer 同步插件：同一选集的多路码流实例共享统一进度。
 *
 * - 任一实例的 play/pause/seek/倍速变化会传播到同组其它实例；
 * - 新加入的实例向已有实例的当前进度看齐（而不是把其它实例拉回 0）；
 * - 首个实例充当时间基准（master），其余实例周期性校正漂移；
 * - 实例销毁自动退出同步组，最后一个成员退出时组随之销毁。
 */

interface SyncState {
  time: number;
  playing: boolean;
  rate: number;
  members: Set<Artplayer>;
  master: Artplayer | null;
}

const groups = new Map<string, SyncState>();

/** 播放器设置持久化（与 WatchPage 共用键）。 */
const PLAYER_SETTINGS_KEY = "jxxk-player-settings";
function loadPersistedRate(): number {
  try {
    const raw = localStorage.getItem(PLAYER_SETTINGS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as { rate?: unknown };
      if (typeof parsed.rate === "number" && parsed.rate > 0) return parsed.rate;
    }
  } catch {
    // 忽略
  }
  return 1;
}
function persistRate(rate: number): void {
  try {
    const raw = localStorage.getItem(PLAYER_SETTINGS_KEY);
    const parsed = raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
    localStorage.setItem(PLAYER_SETTINGS_KEY, JSON.stringify({ ...parsed, rate }));
  } catch {
    // 忽略
  }
}

function groupOf(groupId: string): SyncState {
  let state = groups.get(groupId);
  if (!state) {
    state = {
      time: 0,
      playing: false,
      rate: loadPersistedRate(),
      members: new Set(),
      master: null,
    };
    groups.set(groupId, state);
  }
  return state;
}

// 每个实例的漂移定时器句柄（挂 WeakMap，避免污染 Artplayer 类型）。
const driftTimers = new WeakMap<Artplayer, number>();

function clearDrift(art: Artplayer): void {
  const timer = driftTimers.get(art);
  if (timer !== undefined) {
    window.clearInterval(timer);
    driftTimers.delete(art);
  }
}

/** 生成一个加入指定同步组的插件工厂（经 ArtPlayer 的 plugins 选项挂载）。 */
export function createSyncPlugin(groupId: string): (art: Artplayer) => {
  name: string;
  install: (art: Artplayer) => void;
} {
  return () => ({
    name: "jxxkSync",
    install(art: Artplayer) {
      const state = groupOf(groupId);
      const video = art.video as HTMLVideoElement | undefined;
      if (!video) return;

      // 首个成员成为时间基准。
      if (!state.master) state.master = art;
      state.members.add(art);

      const follow = (time: number, playing: boolean) => {
        if (Math.abs(video.currentTime - time) > 0.35) {
          video.currentTime = time;
        }
        if (playing && video.paused) {
          void art.play().catch(() => {});
        } else if (!playing && !video.paused) {
          art.pause();
        }
        if (video.playbackRate !== state.rate) {
          video.playbackRate = state.rate;
        }
      };

      // 新实例向已有进度看齐。
      const join = () => {
        if (state.time > 0.2) {
          follow(state.time, state.playing);
        }
      };
      join();
      video.addEventListener("loadedmetadata", join, { once: true });

      const onPlay = () => {
        state.playing = true;
        for (const other of state.members) {
          if (other === art) continue;
          const v = other.video as HTMLVideoElement | undefined;
          if (v?.paused) void other.play().catch(() => {});
        }
      };
      const onPause = () => {
        state.playing = false;
        for (const other of state.members) {
          if (other === art) continue;
          const v = other.video as HTMLVideoElement | undefined;
          if (v && !v.paused) other.pause();
        }
      };
      const onSeeked = () => {
        state.time = video.currentTime;
        for (const other of state.members) {
          if (other === art) continue;
          const v = other.video as HTMLVideoElement | undefined;
          if (v && Math.abs(v.currentTime - video.currentTime) > 0.3) {
            v.currentTime = video.currentTime;
          }
        }
      };
      const onRateChange = () => {
        state.rate = video.playbackRate;
        persistRate(video.playbackRate);
        for (const other of state.members) {
          if (other === art) continue;
          const v = other.video as HTMLVideoElement | undefined;
          if (v && v.playbackRate !== video.playbackRate) {
            v.playbackRate = video.playbackRate;
          }
        }
      };
      // master 的 timeupdate 作为共享时钟；非 master 周期性校正漂移。
      const onTimeUpdate = () => {
        if (state.master === art) {
          state.time = video.currentTime;
          return;
        }
        if (!state.playing || state.master === null) return;
        const mv = state.master.video as HTMLVideoElement | undefined;
        if (mv && Math.abs(video.currentTime - mv.currentTime) > 1.2) {
          video.currentTime = mv.currentTime;
        }
      };

      video.addEventListener("play", onPlay);
      video.addEventListener("pause", onPause);
      video.addEventListener("seeked", onSeeked);
      video.addEventListener("ratechange", onRateChange);
      video.addEventListener("timeupdate", onTimeUpdate);
      art.on("ready", join);

      // 非主实例用低频校正定时器兜底（timeupdate 在暂停/缓冲时可能停滞）。
      if (state.master !== art) {
        const timer = window.setInterval(onTimeUpdate, 3000);
        driftTimers.set(art, timer);
      }

      art.on("destroy", () => {
        video.removeEventListener("play", onPlay);
        video.removeEventListener("pause", onPause);
        video.removeEventListener("seeked", onSeeked);
        video.removeEventListener("ratechange", onRateChange);
        video.removeEventListener("timeupdate", onTimeUpdate);
        clearDrift(art);
        state.members.delete(art);
        if (state.master === art) {
          state.master = state.members.values().next().value ?? null;
        }
        if (state.members.size === 0) {
          groups.delete(groupId);
        }
      });
    },
  });
}
