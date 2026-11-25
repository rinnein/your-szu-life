/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { listen } from "@tauri-apps/api/event";
import { PairHostPanel } from "./HostPanel";
import * as api from "./api";
import type { PairHostView } from "./types";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("qrcode.react", () => ({
  QRCodeSVG: ({ value }: { value: string }) => (
    <svg data-testid="pair-qr" data-value={value} aria-hidden="true" />
  ),
}));
vi.mock("./api", () => ({
  pairLoginHostStart: vi.fn(),
  pairLoginHostCancel: vi.fn(),
  pairLoginTakeCredentials: vi.fn(),
}));

const initial: PairHostView = {
  status: "waiting",
  code: "A1B2C3",
  deepLink: "your-szu-life://page/pair-login?secret=" + "a".repeat(64),
  expiresAtMs: 1,
};

type HostEvent = { payload: PairHostView };
let emit: ((event: HostEvent) => void) | undefined;

beforeEach(() => {
  emit = undefined;
  vi.clearAllMocks();
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    emit = handler as (event: HostEvent) => void;
    return () => undefined;
  });
  vi.mocked(api.pairLoginHostCancel).mockResolvedValue(undefined);
  vi.mocked(api.pairLoginTakeCredentials).mockResolvedValue(null);
  vi.mocked(api.pairLoginHostStart).mockResolvedValue(initial);
});

afterEach(cleanup);

function renderPanel(onAuthorized = vi.fn(), onCancel = vi.fn()) {
  render(<PairHostPanel initial={initial} onCancel={onCancel} onAuthorized={onAuthorized} />);
  return { onAuthorized, onCancel };
}

describe("PairHostPanel", () => {
  it("shows the pairing code, QR payload and the waiting state", async () => {
    renderPanel();

    expect(screen.getByText("A1B2C3")).toBeInTheDocument();
    expect(screen.getByTestId("pair-qr")).toHaveAttribute("data-value", initial.deepLink);
    expect(screen.getByText(initial.deepLink)).toBeInTheDocument();
    expect(screen.getByText("等待已登录设备扫码…")).toBeInTheDocument();
  });

  it("switches to connected once the authorize device pulls its info", async () => {
    renderPanel();
    await waitFor(() => expect(emit).toBeDefined());

    act(() => emit?.({ payload: { ...initial, status: "connected" } }));

    expect(screen.getByText("已连接，等待确认登录")).toBeInTheDocument();
  });

  it("takes the credentials exactly once when the authorize device confirms", async () => {
    const credentials = { campusCardNumber: "123456", password: "password-abcd" };
    vi.mocked(api.pairLoginTakeCredentials).mockResolvedValue(credentials);
    const { onAuthorized } = renderPanel();
    await waitFor(() => expect(emit).toBeDefined());

    await act(async () => {
      emit?.({ payload: { ...initial, status: "accepted" } });
    });

    await waitFor(() => expect(onAuthorized).toHaveBeenCalledWith(credentials));
    expect(api.pairLoginTakeCredentials).toHaveBeenCalledTimes(1);
  });

  it("cancels the host session before returning to the manual form", async () => {
    const { onCancel } = renderPanel();

    fireEvent.click(screen.getByRole("button", { name: "取消，改为手动填写" }));

    await waitFor(() => expect(api.pairLoginHostCancel).toHaveBeenCalledTimes(1));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("regenerates an expired pair", async () => {
    render(
      <PairHostPanel
        initial={{ ...initial, status: "expired" }}
        onCancel={vi.fn()}
        onAuthorized={vi.fn()}
      />,
    );

    expect(screen.getByText("配对已过期")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /重新生成/ }));

    await waitFor(() => expect(api.pairLoginHostStart).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("等待已登录设备扫码…")).toBeInTheDocument();
  });
});
