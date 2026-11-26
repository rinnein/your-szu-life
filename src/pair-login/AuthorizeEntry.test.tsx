/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { PairAuthorizeEntry } from "./AuthorizeEntry";
import * as api from "./api";
import { scanQrOnce } from "../qr-scan";
import type { Async } from "../async-state";
import { typeInto } from "../ui/testing";

const auth = vi.hoisted(() => ({
  state: { Authenticated: { name: "测试用户" } } as Record<string, unknown>,
}));

const camera = vi.hoisted(() => ({
  state: { status: "ready", data: true } as Async<boolean>,
}));

const navigation = vi.hoisted(() => ({
  push: vi.fn(),
  back: vi.fn(),
  replace: vi.fn(),
}));

vi.mock("../auth", () => ({ useAuth: () => auth }));
vi.mock("../logical-navigation", () => ({ useLogicalNavigation: () => navigation }));
vi.mock("../qr-scan", () => ({
  scanQrOnce: vi.fn(),
  useCameraAvailability: () => camera.state,
  isScanCancelled: (cause: unknown) =>
    typeof cause === "object" && cause !== null && "code" in cause
      ? cause.code === "cancelled"
      : false,
}));
vi.mock("./api", () => ({ pairLoginAcceptUrl: vi.fn() }));

const invite = "your-szu-life://page/pair-login?secret=" + "a".repeat(64);

beforeEach(() => {
  vi.clearAllMocks();
  auth.state = { Authenticated: { name: "测试用户" } };
  camera.state = { status: "ready", data: true };
  vi.mocked(api.pairLoginAcceptUrl).mockResolvedValue(undefined);
  vi.mocked(scanQrOnce).mockResolvedValue(invite);
});

afterEach(cleanup);

describe("PairAuthorizeEntry", () => {
  it("accepts a pasted invite and opens the authorize page without the secret", async () => {
    render(<PairAuthorizeEntry />);

    fireEvent.click(screen.getByRole("button", { name: "开始授权" }));
    const field = await screen.findByLabelText("配对链接");
    typeInto(field, invite);
    fireEvent.click(screen.getByRole("button", { name: "继续" }));

    await waitFor(() => expect(api.pairLoginAcceptUrl).toHaveBeenCalledWith(invite));
    expect(navigation.push).toHaveBeenCalledWith("/pair-login");
  });

  it("scans the QR whenever a camera is available", async () => {
    render(<PairAuthorizeEntry />);

    fireEvent.click(screen.getByRole("button", { name: "开始授权" }));
    fireEvent.click(await screen.findByRole("button", { name: "扫描二维码" }));

    await waitFor(() => expect(scanQrOnce).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(api.pairLoginAcceptUrl).toHaveBeenCalledWith(invite));
    expect(navigation.push).toHaveBeenCalledWith("/pair-login");
  });

  it("falls back to pasting when no camera is detected", async () => {
    camera.state = { status: "ready", data: false };
    render(<PairAuthorizeEntry />);

    fireEvent.click(screen.getByRole("button", { name: "开始授权" }));

    expect(await screen.findByText("未检测到摄像头，可粘贴配对链接。")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "扫描二维码" })).not.toBeInTheDocument();
  });

  it("shows the backend error inside the dialog", async () => {
    vi.mocked(api.pairLoginAcceptUrl).mockRejectedValueOnce("配对密钥格式无效");
    render(<PairAuthorizeEntry />);

    fireEvent.click(screen.getByRole("button", { name: "开始授权" }));
    const field = await screen.findByLabelText("配对链接");
    typeInto(field, "/pair-login?secret=broken");
    fireEvent.click(screen.getByRole("button", { name: "继续" }));

    expect(await screen.findByText("配对密钥格式无效")).toBeInTheDocument();
    expect(navigation.push).not.toHaveBeenCalled();
  });

  it("is disabled when the local device is not signed in", async () => {
    auth.state = { Unauthenticated: null };
    render(<PairAuthorizeEntry />);

    expect(screen.getByRole("button", { name: "开始授权" })).toBeDisabled();
    expect(screen.getByText("需要先在本机完成统一认证。")).toBeInTheDocument();
  });
});
