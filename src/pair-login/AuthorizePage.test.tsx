/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { useRouter } from "@tanstack/react-router";
import { PairAuthorizePage } from "./AuthorizePage";
import * as api from "./api";
import type { PairPeerView } from "./types";

const navigation = vi.hoisted(() => ({
  back: vi.fn(),
  push: vi.fn(),
  replace: vi.fn(),
}));

vi.mock("@tanstack/react-router", () => ({
  useRouter: vi.fn(),
}));
vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => navigation,
  nativeRuntime: () => false,
  isChildDocument: () => false,
}));
vi.mock("../page-back", () => ({
  usePageBackHandler: () => undefined,
}));
vi.mock("./api", () => ({
  pairLoginConnect: vi.fn(),
  pairLoginAuthorize: vi.fn(),
  pairLoginCancel: vi.fn(),
}));

const peer: PairPeerView = {
  code: "A1B2C3",
  device: { name: "Pixel 7", platform: "android", appVersion: "0.18.0" },
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useRouter).mockReturnValue({
    history: { back: vi.fn() },
  } as unknown as ReturnType<typeof useRouter>);
  vi.mocked(api.pairLoginConnect).mockResolvedValue(peer);
  vi.mocked(api.pairLoginAuthorize).mockResolvedValue(undefined);
  vi.mocked(api.pairLoginCancel).mockResolvedValue(undefined);
  navigation.back.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("PairAuthorizePage", () => {
  it("connects, shows the new device and sends credentials after confirmation", async () => {
    render(<PairAuthorizePage />);

    expect(await screen.findByText("确认授权登录")).toBeInTheDocument();
    expect(screen.getByText("Pixel 7")).toBeInTheDocument();
    expect(screen.getByText(/Android/)).toBeInTheDocument();
    expect(screen.getByText("A1B2C3")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确认授权" }));

    await waitFor(() => expect(api.pairLoginAuthorize).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("已授权")).toBeInTheDocument();
  });

  it("shows a retry path when the new device cannot be reached", async () => {
    vi.mocked(api.pairLoginConnect)
      .mockRejectedValueOnce("无法连接到新设备，请确认两台设备都能访问互联网后重试")
      .mockResolvedValueOnce(peer);

    render(<PairAuthorizePage />);

    expect(await screen.findByText("授权未完成")).toBeInTheDocument();
    expect(screen.getByText(/无法连接到新设备/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重试连接" }));

    expect(await screen.findByText("确认授权登录")).toBeInTheDocument();
    expect(api.pairLoginConnect).toHaveBeenCalledTimes(2);
  });

  it("keeps the connection and offers to resend when authorization fails", async () => {
    vi.mocked(api.pairLoginAuthorize).mockRejectedValueOnce("新设备拒绝了授权（HTTP 409）");

    render(<PairAuthorizePage />);
    expect(await screen.findByText("确认授权登录")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确认授权" }));

    expect(await screen.findByText("授权未完成")).toBeInTheDocument();
    expect(screen.getByText(/HTTP 409/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新发送" }));
    await waitFor(() => expect(api.pairLoginAuthorize).toHaveBeenCalledTimes(2));
  });

  it("cancels the client session when the user leaves", async () => {
    render(<PairAuthorizePage />);
    await screen.findByText("确认授权登录");

    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    await waitFor(() => expect(api.pairLoginCancel).toHaveBeenCalled());
    expect(navigation.back).toHaveBeenCalled();
  });
});
