// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { typeInto } from "../ui/testing";
import { DormStep } from "./DormStep";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const building = ["文韬楼", "6", "-", "12"].join("");
const options = [
  {
    id: "yuehai",
    name: "粤海校区",
    areas: [
      {
        id: "yuehai-main",
        name: "粤海宿舍",
        campusId: "yuehai",
        provider: "sims",
        buildings: [{ id: "b1", name: building }],
      },
    ],
  },
];
const result = {
  campusId: "yuehai",
  areaId: "yuehai-main",
  buildingName: building,
  roomName: "601",
  beginDate: "2026-09-01",
  endDate: "2026-10-01",
  currentRemaining: 10,
  readings: [],
  summary: {
    totalUsed: null,
    dailyAverage: null,
    validDays: 0,
    predictionDailyAverage: null,
    estimatedDaysLeft: null,
    observedDate: null,
    expired: false,
    warningReason: null,
  },
  recharges: [],
  rechargeError: null,
};

function mock(extra: (command: string) => unknown = () => undefined) {
  vi.mocked(invoke).mockImplementation(async (command) => {
    const override = extra(command);
    if (override !== undefined) return override;
    if (command === "get_electricity_options") return options;
    if (command === "get_electricity_settings") return null;
    if (command === "query_electricity") return result;
    if (command === "get_notification_settings") return { electricity: false };
    return null;
  });
}

async function fillDorm() {
  const room = await waitFor(() => {
    const field = screen.getByRole("textbox", { name: "宿舍号" });
    expect(field).toBeEnabled();
    return field;
  });
  typeInto(room, "601");
  const campus = () =>
    screen.queryByRole("button", { name: /校区/ }) ??
    screen.getByRole("combobox", { name: /校区/ });
  await waitFor(() => expect(campus()).toBeEnabled());
  fireEvent.click(campus());
  fireEvent.click(await screen.findByRole("option", { name: "粤海校区" }));
  const save = await screen.findByRole("button", { name: "保存并继续" });
  await waitFor(() => expect(save).toBeEnabled());
  fireEvent.click(save);
  fireEvent.click(await screen.findByRole("button", { name: "确认并保存" }));
}

beforeEach(() => {
  vi.clearAllMocks();
  mock();
});
afterEach(cleanup);

it("persists the dorm only after the query returns data", async () => {
  const onComplete = vi.fn();
  render(<DormStep onComplete={onComplete} onSkip={vi.fn()} />);
  await fillDorm();
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_electricity_settings", {
      settings: {
        campusId: "yuehai",
        areaId: "yuehai-main",
        buildingId: "b1",
        buildingName: building,
        roomName: "601",
      },
    }),
  );
  expect(onComplete).toHaveBeenCalled();
});

it("keeps the dorm unsaved when the query returns no data", async () => {
  mock((command) =>
    command === "query_electricity"
      ? { ...result, currentRemaining: null, readings: [] }
      : undefined,
  );
  const onComplete = vi.fn();
  render(<DormStep onComplete={onComplete} onSkip={vi.fn()} />);
  await fillDorm();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("query_electricity", expect.anything()));
  expect(
    vi.mocked(invoke).mock.calls.some(([command]) => command === "save_electricity_settings"),
  ).toBe(false);
  expect(onComplete).not.toHaveBeenCalled();
});

it("keeps typed characters when only the native input event fires", async () => {
  render(<DormStep onComplete={vi.fn()} onSkip={vi.fn()} />);
  const room = await waitFor(() => {
    const field = screen.getByRole("textbox", { name: "宿舍号" }) as HTMLInputElement;
    expect(field).toBeEnabled();
    return field;
  });
  // 真实浏览器里键入只会触发原生 input；change 要等失焦才触发。
  const typeNative = (value: string) => {
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(room, value);
      room.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
    });
  };
  typeNative("6");
  await waitFor(() => expect(room).toHaveValue("6"));
  typeNative("60");
  await waitFor(() => expect(room).toHaveValue("60"));
  typeNative("601");
  await waitFor(() => expect(room).toHaveValue("601"));
  expect(screen.getByRole("textbox", { name: "所在楼层" })).toHaveValue("6 层");
});
