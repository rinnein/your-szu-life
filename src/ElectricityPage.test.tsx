// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { typeInto } from "./ui/testing";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi, type MockInstance } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "./ui";
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
    if (command === "get_electricity_constants") return { yuanPerKwh: 0.69979 };
    if (command === "get_electricity_options") return options;
    if (command === "get_electricity_settings") return saved;
    if (command === "query_electricity") return result;
    if (command === "export_electricity_settings") return "配置码";
    return null;
  });
}
let successToast: MockInstance;
let dangerToast: MockInstance;
let warningToast: MockInstance;

beforeEach(() => {
  vi.clearAllMocks();
  mock();
  successToast = vi.spyOn(toast, "success").mockReturnValue("toast");
  dangerToast = vi.spyOn(toast, "danger").mockReturnValue("toast");
  warningToast = vi.spyOn(toast, "warning").mockReturnValue("toast");
});
afterEach(cleanup);
it("requests backend-owned dates, shows converted balance and preserves actual recharge amount", async () => {
  render(<CampusPaymentsElectricityPage />);
  expect(await screen.findByText("折算余额 7.00 元")).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("query_electricity", { input: saved });
  expect(screen.queryByLabelText("查询日期范围")).not.toBeInTheDocument();
  expect(screen.getByRole("grid", { name: "充值记录" })).toHaveTextContent("8.50");
  fireEvent.click(screen.getByRole("button", { name: "切换为电费" }));
  expect(screen.getByRole("grid", { name: "用电记录" })).toHaveTextContent("1.40");
});
it("copies exported configuration and offers manual fallback on clipboard failure", async () => {
  vi.mocked(copyText).mockResolvedValueOnce(true).mockResolvedValueOnce(false);
  render(<CampusPaymentsElectricityPage />);
  const button = await screen.findByRole("button", { name: "导出宿舍配置并复制" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
  await waitFor(() => expect(successToast).toHaveBeenCalledWith("宿舍配置已复制"));
  expect(copyText).toHaveBeenCalledWith("配置码");
  fireEvent.click(button);
  await waitFor(() =>
    expect(warningToast).toHaveBeenCalledWith("无法自动复制，请在下方配置码手动复制"),
  );
  expect(await screen.findByLabelText("可复制的宿舍配置码")).toHaveValue("配置码");
});
it("imports into the draft without saving, rejects invalid imports without replacing the draft", async () => {
  const imported = { ...saved, roomName: "602" };
  mock((command) => (command === "import_electricity_settings" ? imported : undefined));
  render(<CampusPaymentsElectricityPage />);
  fireEvent.click(await screen.findByRole("button", { name: "修改宿舍配置" }));
  typeInto(screen.getByLabelText("宿舍配置码"), "配置码");
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "快捷导入宿舍配置" })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "快捷导入宿舍配置" }));
  await waitFor(() =>
    expect(successToast).toHaveBeenCalledWith("已填入宿舍配置，点击查询电费保存并查询"),
  );
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
  await waitFor(() =>
    expect(dangerToast).toHaveBeenCalledWith("导入失败，请检查配置码", {
      description: "配置码格式错误",
    }),
  );
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
    // 宿舍号先填，楼层识别完成后才会放开位置选择。
    const room = await waitFor(() => {
      const field = screen.getByRole("textbox", { name: "宿舍号" });
      expect(field).toBeEnabled();
      return field;
    });
    const floor = screen.getByRole("textbox", { name: "所在楼层" });
    const findCampus = () =>
      screen.queryByRole("button", { name: /校区/ }) ??
      screen.getByRole("combobox", { name: /校区/ });
    // 宿舍号没填或格式不对时，位置选择保持禁用。
    expect(findCampus()).toBeDisabled();
    typeInto(room, "601");
    await waitFor(() => expect(findCampus()).toBeEnabled());
    fireEvent.click(findCampus());
    fireEvent.click(await screen.findByRole("option", { name: campusName }));
    for (const [roomName, expected] of [
      ["601", "6 层"],
      ["1201", "12 层"],
    ]) {
      typeInto(room, roomName);
      expect(floor).toHaveValue(expected);
      expect(screen.getByRole("button", { name: "查询电费" })).toBeEnabled();
    }
    for (const invalid of ["51", "50001", "5A01", "１２０１"]) {
      typeInto(room, invalid);
      expect(floor).toHaveValue("");
      expect(screen.getByText("房间号应为一至两位楼层加两位房间号。")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "查询电费" })).toBeDisabled();
    }
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "query_electricity")).toBe(
      false,
    );
    typeInto(room, "1201");
    fireEvent.click(screen.getByRole("button", { name: "查询电费" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("query_electricity", {
        input: { campusId, areaId, buildingId: "b1", buildingName: "测试楼栋", roomName: "1201" },
      }),
    );
  },
);
it("filters campuses and buildings by the inferred floor and auto-selects single children", async () => {
  const lowRise = ["山茶斋", "1", "-", "5"].join("");
  const highRise = ["文韬楼", "6", "-", "12"].join("");
  const otherRise = ["云杉轩", "8", "-", "10"].join("");
  const roomNumber = "601";
  mock((command) => {
    if (command === "get_electricity_settings") return null;
    if (command === "get_electricity_options")
      return [
        {
          id: "yuehai",
          name: "粤海校区",
          areas: [
            {
              id: "yuehai-main",
              name: "粤海宿舍",
              campusId: "yuehai",
              provider: "sims",
              buildings: [
                { id: "b1", name: lowRise },
                { id: "b2", name: highRise },
              ],
            },
          ],
        },
        {
          id: "xili",
          name: "西丽校区",
          areas: [
            {
              id: "xili-main",
              name: "丽湖宿舍",
              campusId: "xili",
              provider: "sims",
              buildings: [{ id: "b3", name: otherRise }],
            },
          ],
        },
      ];
    return undefined;
  });
  render(<CampusPaymentsElectricityPage />);
  const room = await waitFor(() => {
    const field = screen.getByRole("textbox", { name: "宿舍号" });
    expect(field).toBeEnabled();
    return field;
  });
  typeInto(room, roomNumber);
  const campus = () =>
    screen.queryByRole("button", { name: /校区/ }) ??
    screen.getByRole("combobox", { name: /校区/ });
  await waitFor(() => expect(campus()).toBeEnabled());
  fireEvent.click(campus());
  expect(await screen.findByRole("option", { name: "粤海校区" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "西丽校区" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("option", { name: "粤海校区" }));
  // 区域只有唯一选项，已自动选中；楼栋有两个，保持未选中。
  const building = () =>
    screen.queryByRole("button", { name: /楼栋/ }) ??
    screen.getByRole("combobox", { name: "楼栋" });
  await waitFor(() => expect(building()).toBeEnabled());
  fireEvent.click(building());
  expect(await screen.findByRole("option", { name: highRise })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: lowRise })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("option", { name: highRise }));
  fireEvent.click(screen.getByRole("button", { name: "查询电费" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("query_electricity", {
      input: {
        campusId: "yuehai",
        areaId: "yuehai-main",
        buildingId: "b2",
        buildingName: highRise,
        roomName: roomNumber,
      },
    }),
  );
});
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
  typeInto(screen.getByLabelText("宿舍配置码"), "新配置");
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
it("refreshes from the form and forces a refresh from the details", async () => {
  render(<CampusPaymentsElectricityPage />);
  const button = await screen.findByRole("button", { name: "刷新数据" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("query_electricity", { input: saved }));
  const force = await screen.findByRole("button", { name: "强制刷新" });
  fireEvent.click(force);
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
  typeInto(room, " 1201 ");
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

it("keeps the dorm config unsaved when the query returns no data", async () => {
  mock((command) => {
    if (command === "get_electricity_settings") return { ...saved, roomName: "51" };
    if (command === "query_electricity") return { ...result, currentRemaining: null, readings: [] };
    return undefined;
  });
  render(<CampusPaymentsElectricityPage />);
  const room = await screen.findByRole("textbox", { name: "宿舍号" });
  await waitFor(() => expect(room).toBeEnabled());
  typeInto(room, "1201");
  fireEvent.click(screen.getByRole("button", { name: "查询电费" }));
  expect(
    await screen.findByText("未查询到该宿舍的电费信息，请检查宿舍信息后重试。配置尚未保存。"),
  ).toBeInTheDocument();
  await waitFor(() =>
    expect(warningToast).toHaveBeenCalledWith("未查询到该宿舍的电费信息", expect.anything()),
  );
  expect(
    vi.mocked(invoke).mock.calls.some(([command]) => command === "save_electricity_settings"),
  ).toBe(false);
});

it("offers a retry when room options fail without a saved config", async () => {
  let fail = true;
  mock((command) => {
    if (command === "get_electricity_settings") return null;
    if (command === "get_electricity_options") {
      if (fail) return Promise.reject("校园网暂不可用");
      return options;
    }
    return undefined;
  });
  render(<CampusPaymentsElectricityPage />);
  expect(await screen.findByText("宿舍选项加载失败")).toBeInTheDocument();
  expect(screen.getByText(/校园网暂不可用/)).toBeInTheDocument();
  fail = false;
  fireEvent.click(screen.getByRole("button", { name: "重试" }));
  await waitFor(() => {
    const field = screen.getByRole("textbox", { name: "宿舍号" });
    expect(field).toBeEnabled();
  });
  expect(screen.queryByText("宿舍选项加载失败")).not.toBeInTheDocument();
});

it("isolates rate conversion failure from query data", async () => {
  // getElectricityConstants 有模块级 Promise 缓存；重置模块以获得未命中的新实例。
  vi.resetModules();
  mock((command) =>
    command === "get_electricity_constants" ? Promise.reject("常量不可用") : undefined,
  );
  const { default: FreshPage } = await import("./CampusPaymentsElectricityPage");
  render(<FreshPage />);
  expect(await screen.findByText("电价换算不可用")).toBeInTheDocument();
  expect(screen.getByLabelText("当前电量")).toHaveTextContent("10.00");
  expect(screen.queryByText("查询未完成")).not.toBeInTheDocument();
});

it("keeps the last result visible while editing the draft", async () => {
  render(<CampusPaymentsElectricityPage />);
  await screen.findByLabelText("当前电量");
  fireEvent.click(screen.getByRole("button", { name: "修改宿舍配置" }));
  const room = await waitFor(() => {
    const field = screen.getByRole("textbox", { name: "宿舍号" });
    expect(field).toBeEnabled();
    return field;
  });
  typeInto(room, "602");
  // 结果保留供对照，不因键入被清空。
  expect(screen.getByLabelText("当前电量")).toBeInTheDocument();
});

it("restores the saved config when collapsing the editor", async () => {
  render(<CampusPaymentsElectricityPage />);
  await screen.findByLabelText("当前电量");
  fireEvent.click(screen.getByRole("button", { name: "修改宿舍配置" }));
  const room = await waitFor(() => {
    const field = screen.getByRole("textbox", { name: "宿舍号" });
    expect(field).toBeEnabled();
    return field;
  });
  typeInto(room, "602");
  fireEvent.click(screen.getByRole("button", { name: "收起配置" }));
  expect(screen.getByText("山茶斋 601")).toBeInTheDocument();
  expect(screen.queryByRole("textbox", { name: "宿舍号" })).not.toBeInTheDocument();
  expect(screen.getByLabelText("当前电量")).toBeInTheDocument();
});
