/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { StrictMode } from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { ThemeProvider, useTheme } from "./ThemeProvider";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  getPlatform: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("../os-info", () => ({ getPlatform: mocks.getPlatform }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("./wallpaper", () => ({
  getWallpaperImage: vi.fn().mockRejectedValue(new Error("no wallpaper")),
  seedFromDataUrl: vi.fn(),
}));

const savedAppearance = {
  mode: "system",
  seed: "#5d5fef",
  variant: "tonal-spot",
  seedSource: "wallpaper",
  autoWallpaper: true,
  wallpaperPollMinutes: 15,
};

function Probe() {
  const { appearance } = useTheme();
  return (
    <>
      <span data-testid="auto">{String(appearance.autoWallpaper)}</span>
      <span data-testid="poll">{appearance.wallpaperPollMinutes}</span>
    </>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getPlatform.mockResolvedValue("linux");
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === "get_appearance") return savedAppearance;
    if (command === "get_wallpaper_seed") return { seed: null };
    return null;
  });
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

afterEach(() => {
  cleanup();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

it("applies the saved appearance under StrictMode", async () => {
  render(
    <StrictMode>
      <ThemeProvider>
        <Probe />
      </ThemeProvider>
    </StrictMode>,
  );

  await waitFor(() => expect(screen.getByTestId("auto")).toHaveTextContent("true"));
  expect(screen.getByTestId("poll")).toHaveTextContent("15");
});
