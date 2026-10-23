// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { copyText } from "./clipboard";
import CampusPaymentsElectricityPage from "./CampusPaymentsElectricityPage";
import type { ElectricityResult } from "./electricity";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./clipboard", () => ({ copyText: vi.fn() }));
vi.mock("./logical-navigation", () => ({ useLogicalNavigation: () => ({ replace: vi.fn() }) }));
vi.mock("recharts", async () => {
  const React = await import("react");
  const Container = ({ children }: { children: React.ReactNode }) =>
    React.createElement("div", null, children);
  return {
    ResponsiveContainer: Container,
    ComposedChart: Container,
    Bar: () => null,
    CartesianGrid: () => null,
    Legend: () => null,
    Line: () => null,
    Tooltip: () => null,
    XAxis: () => null,
    YAxis: () => null,
  };
});
const saved = {
  campusId: "yuehai",
  areaId: "yuehai-main",
  buildingId: "b1",
  buildingName: "山茶斋",
  roomName: "601",
};
const options = [
  {
    id: "yuehai",
    name: "粤海",
    areas: [
      {
        id: "yuehai-main",
        name: "粤海宿舍",
        campusId: "yuehai",
        provider: "sims",
        buildings: [{ id: "b1", name: "山茶斋" }],
      },
    ],
  },
];
const result: ElectricityResult = {
  ...saved,
  beginDate: "2026-09-01",
  endDate: "2026-10-01",
  currentRemaining: 10,
  readings: [
    {
      date: "2026-09-30",
      roomName: "601",
      remaining: 10,
      totalUsed: 20,
      totalPurchased: 30,
      dailyUsed: 2,
      dailyPurchased: 0,
    },
  ],
  summary: {
    totalUsed: 20,
    dailyAverage: 2,
    validDays: 10,
    predictionDailyAverage: 2,
    estimatedDaysLeft: 5,
    observedDate: "2026-09-30",
    expired: false,
    warningReason: null,
  },
  recharges: [{ time: "2026-09-29", kwh: 10, yuan: 8.5, source: "微信", sourceKind: "channel" }],
  rechargeError: null,
};
function mock(extra: (command: string) => unknown = () => undefined) {
  vi.mocked(invoke).mockImplementation(async (command) => {
    const override = extra(command);
    if (override !== undefined) return override;
    if (command === "get_electricity_constants") return { yuanPerKwh: 0.61 };
    if (command === "get_electricity_options") return options;
    if (command === "get_electricity_settings") return saved;
    if (command === "query_electricity") return result;
    if (command === "export_electricity_settings") return "配置码";
    return null;
  });
}
beforeEach(() => {
  vi.clearAllMocks();
  mock();
});
afterEach(cleanup);
it("requests backend-owned dates, shows converted balance and preserves actual recharge amount", async () => {
  render(<CampusPaymentsElectricityPage />);
  expect(await screen.findByText("折算余额 6.10 元")).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("query_electricity", { input: saved });
  expect(screen.queryByLabelText("查询日期范围")).not.toBeInTheDocument();
  expect(screen.getByRole("grid", { name: "充值记录" })).toHaveTextContent("8.50");
  fireEvent.click(screen.getByRole("button", { name: "切换为电费" }));
  expect(screen.getByRole("grid", { name: "用电记录" })).toHaveTextContent("1.22");
});
it("copies exported configuration and offers manual fallback on clipboard failure", async () => {
  vi.mocked(copyText).mockResolvedValueOnce(true).mockResolvedValueOnce(false);
  render(<CampusPaymentsElectricityPage />);
  const button = await screen.findByRole("button", { name: "导出宿舍配置并复制" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
  expect(await screen.findByText("宿舍配置已复制。")).toBeInTheDocument();
  expect(copyText).toHaveBeenCalledWith("配置码");
  fireEvent.click(button);
  expect(await screen.findByLabelText("可复制的宿舍配置码")).toHaveValue("配置码");
});
it("imports into the draft without saving, rejects invalid imports without replacing the draft", async () => {
  const imported = { ...saved, roomName: "602" };
  mock((command) => (command === "import_electricity_settings" ? imported : undefined));
  render(<CampusPaymentsElectricityPage />);
  fireEvent.click(await screen.findByRole("button", { name: "修改宿舍配置" }));
  fireEvent.change(screen.getByLabelText("宿舍配置码"), { target: { value: "配置码" } });
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "快捷导入宿舍配置" })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "快捷导入宿舍配置" }));
  await waitFor(() => expect(screen.getByRole("textbox", { name: "宿舍号" })).toHaveValue("602"));
  expect(
    vi.mocked(invoke).mock.calls.some(([command]) => command === "save_electricity_settings"),
  ).toBe(false);
  mock((command) =>
    command === "import_electricity_settings" ? Promise.reject("配置码格式错误") : undefined,
  );
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "快捷导入宿舍配置" })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "快捷导入宿舍配置" }));
  expect(await screen.findByText("配置码格式错误")).toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "宿舍号" })).toHaveValue("602");
});
it.each([
  ["yuehai", "粤海校区", "yuehai-main", "粤海校区宿舍", "sims"],
  ["yuehai", "粤海校区", "yuehai-xinzhai", "深大新斋区", "sims"],
  ["canghai", "沧海校区", "canghai-main", "沧海校区宿舍", "sims"],
  ["xili", "西丽校区", "xili-main", "丽湖校区宿舍", "sims"],
  ["xili", "西丽校区", "xili-lihuo-phase2", "丽湖二期", "lihuoPhase2"],
])(
  "derives floors and validates rooms for %s / %s / %s / %s",
  async (campusId, campusName, areaId, areaName, provider) => {
    mock((command) => {
      if (command === "get_electricity_settings") return null;
      if (command === "get_electricity_options")
        return [
          {
            id: campusId,
            name: campusName,
            areas: [
              {
                id: areaId,
                name: areaName,
                campusId,
                provider,
                buildings: [{ id: "b1", name: "测试楼栋" }],
              },
            ],
          },
        ];
      return undefined;
    });
    render(<CampusPaymentsElectricityPage />);
    await waitFor(() => expect(screen.getByRole("button", { name: /校区/ })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: /校区/ }));
    fireEvent.click(await screen.findByRole("option", { name: campusName }));
    const room = screen.getByRole("textbox", { name: "宿舍号" });
    const floor = screen.getByRole("textbox", { name: "楼层范围" });
    for (const [roomName, expected] of [
      ["601", "6 层"],
      ["1201", "12 层"],
    ]) {
      fireEvent.change(room, { target: { value: roomName } });
      expect(floor).toHaveValue(expected);
      expect(screen.getByRole("button", { name: "查询电费" })).toBeEnabled();
    }
    for (const invalid of ["51", "50001", "5A01", "１２０１"]) {
      fireEvent.change(room, { target: { value: invalid } });
      expect(floor).toHaveValue("");
      expect(screen.getByText("房间号应为一至两位楼层加两位房间号。")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "查询电费" })).toBeDisabled();
    }
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "query_electricity")).toBe(
      false,
    );
    fireEvent.change(room, { target: { value: "1201" } });
    fireEvent.click(screen.getByRole("button", { name: "查询电费" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("query_electricity", {
        input: { campusId, areaId, buildingId: "b1", buildingName: "测试楼栋", roomName: "1201" },
      }),
    );
  },
);
it("retries only recharge data while keeping usage details visible", async () => {
  mock((command) =>
    command === "query_electricity"
      ? { ...result, recharges: [], rechargeError: "充值服务暂不可用" }
      : command === "query_electricity_recharges"
        ? result.recharges
        : undefined,
  );
  render(<CampusPaymentsElectricityPage />);
  fireEvent.click(await screen.findByRole("button", { name: "重试充值记录" }));
  expect(await screen.findByText("充值渠道：微信")).toBeInTheDocument();
  expect(screen.getByLabelText("当前电量")).toHaveTextContent("10.00");
  expect(
    vi.mocked(invoke).mock.calls.filter(([command]) => command === "query_electricity"),
  ).toHaveLength(1);
});
it("does not attach an old recharge retry to an imported room", async () => {
  let resolve!: (value: unknown) => void;
  const pending = new Promise((r) => {
    resolve = r;
  });
  mock((command) =>
    command === "query_electricity"
      ? { ...result, recharges: [], rechargeError: "失败" }
      : command === "query_electricity_recharges"
        ? pending
        : command === "import_electricity_settings"
          ? { ...saved, roomName: "602" }
          : undefined,
  );
  render(<CampusPaymentsElectricityPage />);
  fireEvent.click(await screen.findByRole("button", { name: "重试充值记录" }));
  fireEvent.click(screen.getByRole("button", { name: "修改宿舍配置" }));
  fireEvent.change(screen.getByLabelText("宿舍配置码"), { target: { value: "新配置" } });
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "快捷导入宿舍配置" })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "快捷导入宿舍配置" }));
  await waitFor(() => expect(screen.getByRole("textbox", { name: "宿舍号" })).toHaveValue("602"));
  resolve(result.recharges);
  await waitFor(() =>
    expect(screen.queryByRole("grid", { name: "充值记录" })).not.toBeInTheDocument(),
  );
});

it("renders persisted history before a slow refresh without fetching room options", async () => {
  let resolve!: (value: unknown) => void;
  const pending = new Promise((r) => {
    resolve = r;
  });
  mock((command) =>
    command === "get_cached_electricity"
      ? {
          ...result,
          cache: { fromCache: true, updatedAtMs: Date.UTC(2026, 9, 1), refreshError: null },
        }
      : command === "query_electricity"
        ? pending
        : undefined,
  );
  render(<CampusPaymentsElectricityPage />);
  expect(await screen.findByLabelText("当前电量")).toHaveTextContent("10.00");
  expect(screen.getByText(/已加载本地记录/)).toBeInTheDocument();
  expect(
    vi.mocked(invoke).mock.calls.some(([command]) => command === "get_electricity_options"),
  ).toBe(false);
  await act(async () => resolve({ ...result, currentRemaining: 12 }));
  expect(screen.getByLabelText("当前电量")).toHaveTextContent("12.00");
});
it("keeps local history visible if the refresh fails offline", async () => {
  mock((command) =>
    command === "get_cached_electricity"
      ? result
      : command === "query_electricity"
        ? Promise.reject("校园网暂不可用")
        : undefined,
  );
  render(<CampusPaymentsElectricityPage />);
  expect(await screen.findByText("校园网暂不可用")).toBeInTheDocument();
  expect(screen.getByLabelText("当前电量")).toHaveTextContent("10.00");
  expect(screen.getByRole("grid", { name: "用电记录" })).toBeInTheDocument();
});
it("lets the user explicitly refresh recent data after opening cached details", async () => {
  render(<CampusPaymentsElectricityPage />);
  const button = await screen.findByRole("button", { name: "刷新最新数据" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("query_electricity", { input: saved, forceRefresh: true }),
  );
});

it("normalizes padded saved rooms before querying", async () => {
  mock((command) =>
    command === "get_electricity_settings" ? { ...saved, roomName: " 601 " } : undefined,
  );
  render(<CampusPaymentsElectricityPage />);
  expect(await screen.findByLabelText("当前电量")).toHaveTextContent("10.00");
  expect(invoke).toHaveBeenCalledWith("get_cached_electricity", { input: saved });
  expect(invoke).toHaveBeenCalledWith("query_electricity", { input: saved });
});

it.each(["51", "50001", "5A01", "１２０１", "   "])(
  "opens invalid legacy room %s for editing without querying",
  async (roomName) => {
    mock((command) =>
      command === "get_electricity_settings" ? { ...saved, roomName } : undefined,
    );
    render(<CampusPaymentsElectricityPage />);
    expect(
      await screen.findByText("已保存的宿舍号不符合当前格式，请重新填写后查询。"),
    ).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "宿舍号" })).toHaveValue(roomName.trim());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_electricity_options"));
    expect(screen.getByRole("button", { name: "查询电费" })).toBeDisabled();
    expect(
      vi
        .mocked(invoke)
        .mock.calls.some(([cmd]) =>
          ["get_cached_electricity", "query_electricity", "save_electricity_settings"].includes(
            cmd,
          ),
        ),
    ).toBe(false);
  },
);

it("saves and queries a repaired legacy room in canonical form", async () => {
  mock((command) =>
    command === "get_electricity_settings" ? { ...saved, roomName: "51" } : undefined,
  );
  render(<CampusPaymentsElectricityPage />);
  const room = await screen.findByRole("textbox", { name: "宿舍号" });
  await waitFor(() => expect(room).toBeEnabled());
  fireEvent.change(room, { target: { value: " 1201 " } });
  fireEvent.click(screen.getByRole("button", { name: "查询电费" }));
  const canonical = { ...saved, roomName: "1201" };
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_electricity_settings", { settings: canonical }),
  );
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("query_electricity", { input: canonical }),
  );
  expect(await screen.findByText("山茶斋 1201")).toBeInTheDocument();
});
