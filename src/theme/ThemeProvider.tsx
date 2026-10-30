import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  DEFAULT_APPEARANCE,
  normalizeSeed,
  type Appearance,
  type SchemeVariant,
  type ThemeMode,
} from "./defaults";
import { THEME_NAME, applyThemeCss } from "./tokens";
import { getPlatform } from "../os-info";
import { getWallpaperImage, seedFromDataUrl } from "./wallpaper";

interface ThemeContextValue {
  appearance: Appearance;
  /** `light` 或 `dark`，已解析 `system`。 */
  resolvedMode: "light" | "dark";
  /** 当前壁纸取色是否成功；桌面端用于决定是否显示“动态色彩”按钮。 */
  wallpaperAvailable: boolean;
  /** 当前平台是否支持壁纸变更事件（Android 系统桥）；false 时按间隔轮询。 */
  wallpaperListeningSupported: boolean;
  setMode: (mode: ThemeMode) => void;
  setSeed: (seed: string) => void;
  setVariant: (variant: SchemeVariant) => void;
  /** 桌面端自动壁纸取色开关。 */
  setAutoWallpaper: (enabled: boolean) => void;
  /** 设置无法监听壁纸变更时的轮询间隔（分钟）。 */
  setWallpaperPollMinutes: (minutes: number) => void;
  /** 重新读取系统壁纸颜色并恢复“跟随壁纸”。 */
  followWallpaper: () => Promise<void>;
  /** 重新提取当前壁纸并应用（桌面端），或调用系统动态取色（Android）。 */
  refreshWallpaperSeed: () => Promise<void>;
  /** 恢复默认外观（含跟随系统）。 */
  reset: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false;
}

/** 把解析结果写到 <html>，CSS 与第三方组件（Monaco）都从这里读取。 */
function applyDocumentTheme(resolved: "light" | "dark", mode: ThemeMode) {
  const root = document.documentElement;
  root.dataset.theme = THEME_NAME;
  root.dataset.themeMode = resolved;
  root.style.colorScheme = resolved;
  // index.html 的引导脚本在首帧前读这个值，避免深色模式下闪白。
  try {
    if (mode === "system") localStorage.removeItem("szu.appearance-mode");
    else localStorage.setItem("szu.appearance-mode", mode);
  } catch {
    // localStorage 不可用（隐私模式）时只是少了首帧优化。
  }
}

export function ThemeProvider({
  children,
  initial,
}: {
  children: ReactNode;
  /** 测试与首帧使用；省略时从 Rust 读取。 */
  initial?: Appearance;
}) {
  const [appearance, setAppearance] = useState<Appearance>(initial ?? DEFAULT_APPEARANCE);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);
  const [platform, setPlatform] = useState<string>();
  const [wallpaperAvailable, setWallpaperAvailable] = useState(false);

  const resolvedMode: "light" | "dark" =
    appearance.mode === "system" ? (systemDark ? "dark" : "light") : appearance.mode;

  // 主题 CSS 只在种子色或变体变化时重算——46 个角色走一遍 HCT 不便宜。
  useEffect(() => {
    applyThemeCss(appearance.seed, appearance.variant);
  }, [appearance.seed, appearance.variant]);

  useEffect(() => {
    applyDocumentTheme(resolvedMode, appearance.mode);
  }, [resolvedMode, appearance.mode]);

  // system 模式下跟随操作系统。
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  // 启动时读取已保存的外观。
  //
  // 不要用 ref 做「只读取一次」：StrictMode 会先执行一次挂载、清理、再挂载，
  // ref 在清理后仍然为 true，第二次挂载会直接跳过读取，而第一次读取又已被
  // active=false 丢弃，最终已保存的外观永远不会进入内存。
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;

    void (async () => {
      try {
        const saved = await invoke<Appearance>("get_appearance");
        if (!active) return;
        const seed = normalizeSeed(saved.seed);
        setAppearance({
          ...DEFAULT_APPEARANCE,
          ...saved,
          seed: seed ?? DEFAULT_APPEARANCE.seed,
        });
      } catch {
        // 配置读不出来时保持默认外观，不阻塞启动。
      }
    })();

    return () => {
      active = false;
    };
  }, []);

  // 平台只在启动时解析一次；桌面端自动壁纸取色需要它区分 Android。
  useEffect(() => {
    let active = true;
    void getPlatform()
      .then((value) => {
        if (active) setPlatform(value);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  // 壁纸色优先级最高，但用户显式选过色之后就不再覆盖。
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;

    const refresh = async () => {
      try {
        const wallpaper = await invoke<{ seed: string | null }>("get_wallpaper_seed");
        const seed = normalizeSeed(wallpaper?.seed);
        if (!active || !seed) return;
        setWallpaperAvailable(true);
        setAppearance((current) =>
          current.seedSource === "user" ? current : { ...current, seed, seedSource: "wallpaper" },
        );
      } catch {
        // 桌面平台没有壁纸色，属正常情况。
      }
    };

    void refresh();
    // 原生侧在 onResume 检测到壁纸变化后派发此事件。
    const onWallpaperChange = () => void refresh();
    window.addEventListener("szu-wallpaper-changed", onWallpaperChange);

    return () => {
      active = false;
      window.removeEventListener("szu-wallpaper-changed", onWallpaperChange);
    };
  }, []);

  // 桌面端可选：启动时自动读取当前壁纸并提取种子色，提供类似 Android 的动态色彩。
  useEffect(() => {
    if (!isTauri() || !platform || platform === "android") return;
    if (!appearance.autoWallpaper) {
      setWallpaperAvailable(false);
      return;
    }
    let active = true;
    void (async () => {
      try {
        const image = await getWallpaperImage();
        const seed = normalizeSeed(await seedFromDataUrl(image.dataUrl));
        if (!active || !seed) return;
        setWallpaperAvailable(true);
        setAppearance((current) =>
          current.seedSource === "user" ? current : { ...current, seed, seedSource: "wallpaper" },
        );
      } catch {
        // Wayland 等无法读取壁纸的环境：保持现有配色，隐藏动态色彩按钮。
        if (active) setWallpaperAvailable(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [platform, appearance.autoWallpaper]);

  // 桌面端壁纸管理器通常没有变更事件；按用户设定的分钟间隔轮询。
  useEffect(() => {
    if (!isTauri() || !platform || platform === "android" || !appearance.autoWallpaper) return;
    let active = true;
    const minutes = Math.min(1440, Math.max(1, appearance.wallpaperPollMinutes));
    const timer = window.setInterval(() => {
      void (async () => {
        try {
          const image = await getWallpaperImage();
          const seed = normalizeSeed(await seedFromDataUrl(image.dataUrl));
          if (!active || !seed) return;
          setWallpaperAvailable(true);
          setAppearance((current) => {
            if (current.seedSource === "user" || current.seed === seed) return current;
            return { ...current, seed, seedSource: "wallpaper" };
          });
        } catch {
          // 轮询失败时保留上次成功的配色。
        }
      })();
    }, minutes * 60_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [platform, appearance.autoWallpaper, appearance.wallpaperPollMinutes]);

  // 设置可能从另一个窗口或 MCP 工具改动。
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    let stop: (() => void) | undefined;
    void listen<Appearance>("appearance-changed", (event) => {
      const next = event.payload;
      const seed = normalizeSeed(next?.seed);
      if (seed) setAppearance({ ...DEFAULT_APPEARANCE, ...next, seed });
    })
      .then((off) => {
        if (active) stop = off;
        else off();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      stop?.();
    };
  }, []);

  const persist = useCallback((next: Appearance) => {
    setAppearance(next);
    if (!isTauri()) return;
    void invoke("save_appearance", { appearance: next }).catch(() => undefined);
  }, []);

  const setMode = useCallback(
    (mode: ThemeMode) => persist({ ...appearance, mode }),
    [appearance, persist],
  );

  const setSeed = useCallback(
    (value: string) => {
      const seed = normalizeSeed(value);
      // 非法输入直接忽略，避免把坏值写进配置文件。
      if (!seed) return;
      persist({ ...appearance, seed, seedSource: "user" });
    },
    [appearance, persist],
  );

  const setVariant = useCallback(
    (variant: SchemeVariant) => persist({ ...appearance, variant }),
    [appearance, persist],
  );

  const followWallpaper = useCallback(async () => {
    const wallpaper = await invoke<{ seed: string | null }>("get_wallpaper_seed");
    const seed = normalizeSeed(wallpaper?.seed);
    if (!seed) throw new Error("当前平台无法读取壁纸颜色");
    persist({ ...appearance, seed, seedSource: "wallpaper" });
  }, [appearance, persist]);

  const refreshWallpaperSeed = useCallback(async () => {
    if (platform === "android") {
      await followWallpaper();
      setWallpaperAvailable(true);
      return;
    }
    const image = await getWallpaperImage();
    const seed = normalizeSeed(await seedFromDataUrl(image.dataUrl));
    if (!seed) throw new Error("未能从当前壁纸提取颜色");
    setWallpaperAvailable(true);
    persist({ ...appearance, seed, seedSource: "wallpaper" });
  }, [platform, appearance, persist, followWallpaper]);

  const setAutoWallpaper = useCallback(
    (enabled: boolean) => {
      persist({
        ...appearance,
        autoWallpaper: enabled,
        // 开启时立即把来源切回壁纸，让提取结果可以覆盖当前种子色。
        seedSource: enabled ? "wallpaper" : appearance.seedSource,
      });
    },
    [appearance, persist],
  );

  const setWallpaperPollMinutes = useCallback(
    (minutes: number) => {
      const value = Math.min(1440, Math.max(1, Math.round(minutes)));
      persist({ ...appearance, wallpaperPollMinutes: value });
    },
    [appearance, persist],
  );

  const reset = useCallback(() => persist({ ...DEFAULT_APPEARANCE }), [persist]);

  const value = useMemo(
    () => ({
      appearance,
      resolvedMode,
      wallpaperAvailable,
      wallpaperListeningSupported: platform === "android",
      setMode,
      setSeed,
      setVariant,
      setAutoWallpaper,
      setWallpaperPollMinutes,
      followWallpaper,
      refreshWallpaperSeed,
      reset,
    }),
    [
      appearance,
      resolvedMode,
      wallpaperAvailable,
      platform,
      setMode,
      setSeed,
      setVariant,
      setAutoWallpaper,
      setWallpaperPollMinutes,
      followWallpaper,
      refreshWallpaperSeed,
      reset,
    ],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext);
  if (!context) throw new Error("useTheme 必须在 ThemeProvider 内使用");
  return context;
}
