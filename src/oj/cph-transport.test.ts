import { expect, it, vi } from "vite-plus/test";
import { connectCph, validCphSubmission } from "./cph-transport";
import type { io } from "socket.io-client";
function fixture() {
  const handlers = new Map<string, (...args: any[]) => unknown>();
  const socket = {
    connected: true,
    on: vi.fn((event, callback) => handlers.set(event, callback)),
    emit: vi.fn(),
    removeAllListeners: vi.fn(),
    disconnect: vi.fn(),
  };
  const factory = vi.fn(() => socket) as unknown as typeof io;
  return { socket, factory, event: (name: string, value?: unknown) => handlers.get(name)?.(value) };
}
it("registers as a browser and does not take over another active submitter", async () => {
  const f = fixture();
  const changed = vi.fn(),
    submit = vi.fn().mockResolvedValue({});
  const bridge = connectCph(27121, changed, submit, f.factory);
  expect(f.factory).toHaveBeenCalledWith(
    "http://127.0.0.1:27121",
    expect.objectContaining({ path: "/ws", query: expect.objectContaining({ type: "browser" }) }),
  );
  f.event("connect");
  f.event("status", { isActive: false });
  await f.event("submitRequest", { url: "http://oj.test/", sourceCode: "int main(){}" });
  expect(submit).not.toHaveBeenCalled();
  expect(f.socket.emit).not.toHaveBeenCalled();
  bridge.activate();
  expect(f.socket.emit).toHaveBeenCalledWith("setActive");
  f.event("status", { isActive: true });
  await f.event("submitRequest", { url: "http://oj.test/", sourceCode: "int main(){}" });
  expect(submit).toHaveBeenCalledOnce();
  bridge.disconnect();
  expect(f.socket.disconnect).toHaveBeenCalled();
});
it("rejects malformed and overlapping requests without resubmitting", async () => {
  const f = fixture();
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const submit = vi.fn(() => pending),
    changed = vi.fn();
  connectCph(27121, changed, submit, f.factory);
  f.event("status", { isActive: true });
  expect(validCphSubmission({ url: 1, sourceCode: "x" })).toBe(false);
  f.event("submitRequest", { url: 1, sourceCode: "x" });
  expect(submit).not.toHaveBeenCalled();
  const first = f.event("submitRequest", { url: "http://oj.test/", sourceCode: "x" });
  await f.event("submitRequest", { url: "http://oj.test/", sourceCode: "x" });
  expect(submit).toHaveBeenCalledOnce();
  finish();
  await first;
});
