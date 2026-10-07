/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import CaptchaInput from "./CaptchaInput";
import type { Snapshot } from "./model";
import { snapshotFixture } from "./testing/fixtures";
const { run } = vi.hoisted(() => ({ run: vi.fn() }));
vi.mock("./use-enrollment", () => ({ useEnrollment: () => ({ run, errors: {} }) }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ status: "cancelled" }),
}));
Element.prototype.getAnimations = () => [];
type Pending = { action: Record<string, any>; resolve: (v: unknown) => void };
let requests: Pending[];
let done = vi.fn<(s: Snapshot) => void>();
let imageId = 0;
const pointReply = (p: Pending, point: [number, number]) => ({
  status: "point",
  challengeId: p.action.challengeId,
  revision: p.action.revision,
  index: p.action.points.length,
  point,
});
const submits = () => run.mock.calls.filter(([a]) => a.type === "captchaSubmit");
const points = () => screen.queryAllByLabelText(/^标记 /);
async function resolvePoint(index: number) {
  const pending = requests[index];
  await act(async () => pending.resolve(pointReply(pending, [10 + index * 30, 35])));
}
function click(x: number) {
  fireEvent.click(screen.getByRole("button", { name: "点选验证码" }), { clientX: x, clientY: 50 });
}
beforeEach(() => {
  run.mockReset();
  requests = [];
  done = vi.fn<(s: Snapshot) => void>();
  imageId = 0;
  vi.stubGlobal("requestAnimationFrame", (fn: FrameRequestCallback) => setTimeout(() => fn(0), 0));
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: 250,
    bottom: 80,
    width: 250,
    height: 80,
    toJSON: () => ({}),
  });
  run.mockImplementation(async (a) => {
    if (a.type === "captchaPrepare")
      return {
        status: "challenge",
        challengeId: `image-${++imageId}`,
        image: "data:image/png;base64,iVBORw0KGgo=",
      };
    if (a.type === "captchaPoint")
      return new Promise((resolve) => requests.push({ action: a, resolve }));
    if (a.type === "captchaSubmit")
      return { status: "authenticated", snapshot: { ...snapshotFixture, loggedIn: true } };
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
it.each([0, 1, 2, 3])(
  "preserves %i automatic points when manual input interrupts a late result",
  async (count) => {
    render(<CaptchaInput onAuthenticated={done} />);
    await waitFor(() => expect(requests).toHaveLength(1));
    for (let i = 0; i < count; i++) {
      await resolvePoint(i);
      await waitFor(() => expect(requests).toHaveLength(i + 2));
    }
    expect(points()).toHaveLength(count);
    click(150);
    expect(points()).toHaveLength(count + 1);
    await resolvePoint(count);
    expect(points()).toHaveLength(count + 1);
    for (let i = count + 1; i < 4; i++) click(150 + i * 15);
    await waitFor(() => expect(submits()).toHaveLength(1));
    expect(submits()[0][0].points).toHaveLength(4);
    expect(submits()[0][0].points.slice(0, count)).toEqual(
      Array.from({ length: count }, (_, i) => [10 + i * 30, 35]),
    );
    expect(done).toHaveBeenCalledTimes(1);
  },
);
it("fills one visible point per reply and submits immediately on the fourth", async () => {
  render(<CaptchaInput onAuthenticated={done} />);
  for (let i = 0; i < 4; i++) {
    await waitFor(() => expect(requests).toHaveLength(i + 1));
    expect(points()).toHaveLength(i);
    await resolvePoint(i);
  }
  await waitFor(() => expect(submits()).toHaveLength(1));
  expect(points()).toHaveLength(4);
  expect(screen.getByRole("button", { name: "点选验证码" })).toBeDisabled();
  click(200);
  expect(submits()).toHaveLength(1);
  expect(screen.queryByRole("button", { name: "确认登录" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "手动处理验证码" })).not.toBeInTheDocument();
});
it("resumes after fixed manual points and rejects the previous automatic run", async () => {
  render(<CaptchaInput onAuthenticated={done} />);
  await waitFor(() => expect(requests).toHaveLength(1));
  await resolvePoint(0);
  await waitFor(() => expect(requests).toHaveLength(2));
  click(150);
  fireEvent.click(screen.getByRole("button", { name: "自动识别" }));
  await waitFor(() => expect(requests).toHaveLength(3));
  expect(requests[2].action.points).toEqual([
    [10, 35],
    [150, 50],
  ]);
  await resolvePoint(1);
  expect(points()).toHaveLength(2);
  await resolvePoint(2);
  await waitFor(() => expect(requests).toHaveLength(4));
  await resolvePoint(3);
  await waitFor(() => expect(submits()).toHaveLength(1));
});
it("clears points on replacement and does not resume automatically", async () => {
  render(<CaptchaInput onAuthenticated={done} />);
  await waitFor(() => expect(requests).toHaveLength(1));
  await resolvePoint(0);
  await waitFor(() => expect(requests).toHaveLength(2));
  fireEvent.click(screen.getByRole("button", { name: "换一张" }));
  await screen.findByText("按顶部顺序点选四个字");
  await resolvePoint(1);
  expect(points()).toHaveLength(0);
  expect(requests).toHaveLength(2);
  expect(imageId).toBe(2);
});
it.each([
  ["structural", 3],
  ["uncertain", 5],
])("bounds %s retries and remains available for manual input", async (status, max) => {
  const original = run.getMockImplementation()!;
  run.mockImplementation((a) =>
    a.type === "captchaPoint" ? Promise.resolve({ status }) : original(a),
  );
  render(<CaptchaInput onAuthenticated={done} />);
  await screen.findByText("自动识别未完成，请手动点选");
  expect(imageId).toBe(max);
  expect(screen.getByText(`已重试 ${max - 1} 次`)).toBeVisible();
  expect(screen.getByRole("button", { name: "点选验证码" })).toBeEnabled();
  expect(submits()).toHaveLength(0);
});
it("discards a reply after unmount instead of submitting", async () => {
  const view = render(<CaptchaInput onAuthenticated={done} />);
  await waitFor(() => expect(requests).toHaveLength(1));
  view.unmount();
  await resolvePoint(0);
  expect(submits()).toHaveLength(0);
  expect(requests).toHaveLength(1);
});

it("refreshes a manually rejected challenge without restarting OCR", async () => {
  const original = run.getMockImplementation()!;
  run.mockImplementation((a) =>
    a.type === "captchaSubmit"
      ? Promise.resolve({ status: "rejected", message: "验证码错误" })
      : original(a),
  );
  render(<CaptchaInput onAuthenticated={done} />);
  await waitFor(() => expect(requests).toHaveLength(1));
  for (const x of [10, 50, 100, 150]) click(x);
  await screen.findByText("按顶部顺序点选四个字");
  expect(submits()).toHaveLength(1);
  expect(imageId).toBe(2);
  expect(points()).toHaveLength(0);
  expect(requests).toHaveLength(1);
});
it("does not retry a network failure or password rejection", async () => {
  const original = run.getMockImplementation()!;
  run.mockImplementation((a) =>
    a.type === "captchaSubmit" ? Promise.resolve(undefined) : original(a),
  );
  render(<CaptchaInput onAuthenticated={done} />);
  await waitFor(() => expect(requests).toHaveLength(1));
  for (const x of [10, 50, 100, 150]) click(x);
  await screen.findByText("本次登录未完成，请重新检查登录状态");
  expect(submits()).toHaveLength(1);
  expect(imageId).toBe(1);
  expect(done).not.toHaveBeenCalled();
});
