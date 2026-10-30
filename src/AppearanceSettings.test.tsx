/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { typeInto } from "./ui/testing";
import type { StepNextAction } from "./step-next-action";
import { AppearanceSettings } from "./AppearanceSettings";

const mocks = vi.hoisted(() => ({
  appearance: {
    mode: "system",
    seed: "#5d5fef",
    variant: "tonal-spot",
    seedSource: "default",
    autoWallpaper: false,
    wallpaperPollMinutes: 15,
  },
  wallpaperAvailable: false,
  wallpaperListeningSupported: false,
  setMode: vi.fn(),
  setSeed: vi.fn(),
  setVariant: vi.fn(),
  setAutoWallpaper: vi.fn(),
  setWallpaperPollMinutes: vi.fn(),
  reset: vi.fn(),
  refreshWallpaperSeed: vi.fn(),
  getPlatform: vi.fn(),
}));

vi.mock("./theme/ThemeProvider", () => ({
  useTheme: () => ({
    appearance: mocks.appearance,
    resolvedMode: "light",
    wallpaperAvailable: mocks.wallpaperAvailable,
    wallpaperListeningSupported: mocks.wallpaperListeningSupported,
    setMode: mocks.setMode,
    setSeed: mocks.setSeed,
    setVariant: mocks.setVariant,
    setAutoWallpaper: mocks.setAutoWallpaper,
    setWallpaperPollMinutes: mocks.setWallpaperPollMinutes,
    reset: mocks.reset,
    refreshWallpaperSeed: mocks.refreshWallpaperSeed,
  }),
}));

vi.mock("./os-info", () => ({ getPlatform: mocks.getPlatform }));

function Harness({ onComplete }: { onComplete: () => void | Promise<void> }) {
  const [action, setAction] = useState<StepNextAction | null>(null);
  return (
    <>
      <AppearanceSettings onboarding onComplete={onComplete} registerNext={setAction} />
      {action && (
        <button type="button" disabled={action.disabled} onClick={() => void action.run()}>
          {action.label}
        </button>
      )}
    </>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.wallpaperAvailable = false;
  mocks.wallpaperListeningSupported = false;
  mocks.appearance.autoWallpaper = false;
  mocks.appearance.seed = "#5d5fef";
  mocks.appearance.seedSource = "default";
  mocks.appearance.wallpaperPollMinutes = 15;
  mocks.getPlatform.mockResolvedValue("linux");
  mocks.refreshWallpaperSeed.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("AppearanceSettings", () => {
  it("keeps the full settings surface", async () => {
    render(<AppearanceSettings />);

    expect(screen.getByText("配色变体")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "恢复默认外观" })).toBeInTheDocument();
    expect(screen.queryByText("从壁纸取色")).not.toBeInTheDocument();
    expect(await screen.findByText("壁纸动态色彩")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "自动从壁纸取色" })).toBeInTheDocument();
    expect(screen.getByText("壁纸变更监视为轮询")).toBeInTheDocument();
    expect(screen.getByText("默认")).toBeInTheDocument();
    expect(document.querySelectorAll(".monet-swatch")).toHaveLength(36);
  });

  it("locks the seed section and shows the extracted color when auto wallpaper is on", async () => {
    mocks.appearance.autoWallpaper = true;
    mocks.appearance.seed = "#123456";
    render(<AppearanceSettings />);

    expect(await screen.findByText("种子色已锁定")).toBeInTheDocument();
    expect(screen.getByLabelText("自定义颜色")).toBeDisabled();
    expect(screen.getByLabelText("自定义颜色")).toHaveValue("#123456");
    expect(screen.getByRole("button", { name: "使用靛蓝配色" })).toBeDisabled();
  });

  it("selects a variant from the combined color list", async () => {
    render(<AppearanceSettings />);

    fireEvent.click(await screen.findByRole("radio", { name: "表现力（Expressive）" }));
    await waitFor(() => expect(mocks.setVariant).toHaveBeenCalledWith("expressive"));
  });

  it("toggles automatic wallpaper extraction", async () => {
    render(<AppearanceSettings />);

    fireEvent.click(await screen.findByRole("switch", { name: "自动从壁纸取色" }));
    await waitFor(() => expect(mocks.setAutoWallpaper).toHaveBeenCalledWith(true));
  });

  it("lets the user configure the polling interval when listening is unsupported", async () => {
    mocks.appearance.autoWallpaper = true;
    render(<AppearanceSettings />);

    const input = await screen.findByLabelText("检查间隔（分钟）");
    typeInto(input, "10");
    await waitFor(() => expect(mocks.setWallpaperPollMinutes).toHaveBeenCalledWith(10));
  });

  it("shows a persistent banner and retry action when wallpaper extraction fails", async () => {
    mocks.appearance.autoWallpaper = true;
    render(<AppearanceSettings />);

    expect(await screen.findByText("暂未读取到壁纸颜色")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /重试读取壁纸/ }));
    await waitFor(() => expect(mocks.refreshWallpaperSeed).toHaveBeenCalledTimes(1));
  });

  it("offers the dynamic color button after wallpaper extraction succeeded", async () => {
    mocks.wallpaperAvailable = true;
    render(<AppearanceSettings />);

    fireEvent.click(await screen.findByRole("button", { name: /跟随壁纸动态取色/ }));
    await waitFor(() => expect(mocks.refreshWallpaperSeed).toHaveBeenCalledTimes(1));
  });

  it("registers 下一步 and hides the seed/settings-only blocks in onboarding mode", async () => {
    const onComplete = vi.fn();
    render(<Harness onComplete={onComplete} />);

    expect(await screen.findByText("壁纸动态色彩")).toBeInTheDocument();
    expect(screen.queryByText("种子色")).not.toBeInTheDocument();
    expect(screen.queryByText("自定义颜色")).not.toBeInTheDocument();
    expect(screen.queryByText("配色变体")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "恢复默认外观" })).not.toBeInTheDocument();

    fireEvent.click(await screen.findByRole("button", { name: "下一步" }));
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("offers the dynamic color control in onboarding when wallpaper is available", async () => {
    mocks.wallpaperAvailable = true;
    render(<Harness onComplete={vi.fn()} />);

    fireEvent.click(await screen.findByRole("button", { name: /跟随壁纸动态取色/ }));
    await waitFor(() => expect(mocks.refreshWallpaperSeed).toHaveBeenCalledTimes(1));
  });

  it("shows the dynamic color card on the Android branch", async () => {
    mocks.getPlatform.mockResolvedValue("android");
    mocks.wallpaperListeningSupported = true;
    render(<Harness onComplete={vi.fn()} />);

    expect(await screen.findByText("使用系统壁纸颜色生成 Material You 配色。")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: /跟随壁纸动态取色/ }));
    await waitFor(() => expect(mocks.refreshWallpaperSeed).toHaveBeenCalledTimes(1));
  });
});
