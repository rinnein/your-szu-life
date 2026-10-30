// @vitest-environment jsdom
import { describe, expect, it } from "vite-plus/test";
import {
  filterElectricityBuildings,
  filterElectricityCampuses,
  getElectricityBuildingFloorRange,
  isElectricitySelectionAvailable,
  type ElectricityCampus,
} from "./electricity-location";

describe("getElectricityBuildingFloorRange", () => {
  it("reads the trailing floor range from a building name", () => {
    expect(getElectricityBuildingFloorRange("山茶斋1-7")).toEqual({ from: 1, to: 7 });
    expect(getElectricityBuildingFloorRange("文韬楼 3 - 8 层")).toEqual({ from: 3, to: 8 });
    expect(getElectricityBuildingFloorRange("云杉轩12～15")).toEqual({ from: 12, to: 15 });
  });

  it("returns null when the name has no usable range", () => {
    expect(getElectricityBuildingFloorRange("B3文韬楼")).toBeNull();
    expect(getElectricityBuildingFloorRange("山茶斋9-3")).toBeNull();
    expect(getElectricityBuildingFloorRange("山茶斋0-7")).toBeNull();
  });
});

const campuses: ElectricityCampus[] = [
  {
    id: "c1",
    name: "校区一",
    areas: [
      {
        id: "a1",
        name: "区域一",
        campusId: "c1",
        provider: "sims",
        buildings: [
          { id: "b1", name: "山茶斋1-7" },
          { id: "b2", name: "文韬楼8-12" },
          { id: "b3", name: "未知楼" },
        ],
      },
      {
        id: "a2",
        name: "区域二",
        campusId: "c1",
        provider: "sims",
        buildings: [{ id: "b4", name: "云杉轩8-10" }],
      },
    ],
  },
  {
    id: "c2",
    name: "校区二",
    areas: [
      {
        id: "a3",
        name: "区域三",
        campusId: "c2",
        provider: "sims",
        buildings: [{ id: "b5", name: "木棉楼1-5" }],
      },
    ],
  },
];

describe("floor filtering", () => {
  it("keeps buildings whose range contains the floor and unparsable buildings", () => {
    expect(filterElectricityBuildings(campuses[0].areas[0].buildings, 6).map((b) => b.id)).toEqual([
      "b1",
      "b3",
    ]);
  });

  it("does not filter anything before the room number is valid", () => {
    expect(filterElectricityCampuses(campuses, null)).toBe(campuses);
  });

  it("drops areas whose buildings are all filtered out", () => {
    const filtered = filterElectricityCampuses(campuses, 6);
    expect(filtered.map((c) => c.id)).toEqual(["c1"]);
    expect(filtered[0].areas.map((a) => a.id)).toEqual(["a1"]);
    expect(filtered[0].areas[0].buildings.map((b) => b.id)).toEqual(["b1", "b3"]);
  });

  it("drops a campus when every area under it is filtered out", () => {
    const filtered = filterElectricityCampuses(campuses, 12);
    expect(filtered.map((c) => c.id)).toEqual(["c1"]);
    expect(filtered[0].areas.map((a) => a.id)).toEqual(["a1"]);
  });

  it("keeps areas with no buildings instead of hiding them", () => {
    const empty: ElectricityCampus[] = [
      {
        id: "c3",
        name: "校区三",
        areas: [{ id: "a4", name: "区域四", campusId: "c3", provider: "sims", buildings: [] }],
      },
    ];
    expect(filterElectricityCampuses(empty, 6)[0].areas).toHaveLength(1);
  });
});

describe("isElectricitySelectionAvailable", () => {
  const value = {
    campusId: "c1",
    areaId: "a1",
    buildingId: "b1",
    buildingName: "山茶斋1-7",
    roomName: "601",
  };

  it("accepts a selection that survives floor filtering", () => {
    expect(isElectricitySelectionAvailable(campuses, value)).toBe(true);
  });

  it("rejects a building outside the inferred floor", () => {
    expect(isElectricitySelectionAvailable(campuses, { ...value, buildingId: "b2" })).toBe(false);
  });

  it("rejects an invalid room number", () => {
    expect(isElectricitySelectionAvailable(campuses, { ...value, roomName: "51" })).toBe(false);
  });

  it("does not block saved settings while options are unavailable", () => {
    expect(isElectricitySelectionAvailable([], value)).toBe(true);
  });
});
