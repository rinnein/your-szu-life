import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
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

interface ThemeContextValue {
  appearance: Appearance;
  /** `light` 或 `dark`，已解析 `system`。 */
  resolvedMode: "light" | "dark";
  setMode: (mode: ThemeMode) => void;
  setSeed: (seed: string) => void;
  setVariant: (variant: SchemeVariant) => void;
  /** 重新读取系统壁纸颜色并恢复“跟随壁纸”。 */
  followWallpaper: () => Promise<void>;
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
  const persisted = useRef(false);

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
  useEffect(() => {
    if (persisted.current || !isTauri()) return;
    persisted.current = true;
    let active = true;

    void (async () => {
      try {
        const saved = await invoke<Appearance>("get_appearance");
        if (!active) return;
        const seed = normalizeSeed(saved.seed);
        setAppearance({ ...saved, seed: seed ?? DEFAULT_APPEARANCE.seed });
      } catch {
        // 配置读不出来时保持默认外观，不阻塞启动。
      }
    })();

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

  // 设置可能从另一个窗口或 MCP 工具改动。
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen<Appearance>("appearance-changed", (event) => {
      const next = event.payload;
      const seed = normalizeSeed(next?.seed);
      if (seed) setAppearance({ ...next, seed });
    }).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten?.();
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

  const reset = useCallback(() => persist({ ...DEFAULT_APPEARANCE }), [persist]);

  const value = useMemo(
    () => ({ appearance, resolvedMode, setMode, setSeed, setVariant, followWallpaper, reset }),
    [appearance, resolvedMode, setMode, setSeed, setVariant, followWallpaper, reset],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext);
  if (!context) throw new Error("useTheme 必须在 ThemeProvider 内使用");
  return context;
}
