import { describe, it, expect } from "vite-plus/test";
import { academicFixture } from "../testing/fixtures";
import {
  aggregateCourses,
  completionSummary,
  courseCategory,
  buildTree,
  defaultFilters,
  descendants,
  filterCourses,
  nodePath,
} from "./tree";
import { getViewState, saveViewState, migrateViewState } from "./view-state";
describe("academic tree", () => {
  it("aggregates every descendant at ancestors and deduplicates codes", () => {
    const tree = buildTree(academicFixture);
    expect(aggregateCourses(tree, "plan")).toHaveLength(51);
    const general = aggregateCourses(tree, "group:general");
    expect(general).toHaveLength(48);
    const duplicate = general.find((c) => c.code === "GE001")!;
    expect(duplicate.groups).toHaveLength(2);
    expect(duplicate.conflict).toBe(true);
    expect(duplicate.credits).toContain("4");
    expect(duplicate.statuses).toContain("已完成");
    expect(duplicate.statuses).toContain("已选未完成");
  });
  it("stops at real groups while retaining courses in their query index", () => {
    const tree = buildTree(academicFixture);
    expect(tree.nodes.get("group:major")!.children).toEqual([]);
    expect(aggregateCourses(tree, "group:major")).toHaveLength(2);
    expect(nodePath(tree, "group:major")).toEqual([academicFixture.summary[0].PYFAMC, "专业教育"]);
    expect(descendants(tree, "plan").every((n) => n.kind === "plan" || n.kind === "group")).toBe(
      true,
    );
  });
  it("keeps unknown codes separate and breaks cycles without dropping courses", () => {
    const tree = buildTree({
      ...academicFixture,
      modules: [
        { module: { KZH: "a", FKZH: "b", KZM: "A" }, courses: [{ KCM: "同名课程" }] },
        { module: { KZH: "b", FKZH: "a", KZM: "B" }, courses: [{ KCM: "同名课程" }] },
        { module: { KZH: "c", FKZH: "missing" }, courses: [] },
      ],
    });
    expect(tree.warnings).toHaveLength(2);
    expect(aggregateCourses(tree, "plan")).toHaveLength(2);
  });
  it("searches, filters and sorts the aggregate rather than only visible branches", () => {
    const rows = aggregateCourses(buildTree(academicFixture), "plan");
    expect(filterCourses(rows, { ...defaultFilters, query: "CS101" })).toHaveLength(1);
    expect(
      filterCourses(rows, { ...defaultFilters, group: "group:major", status: "已完成" }).map(
        (c) => c.code,
      ),
    ).toEqual(["CS101"]);
    expect(
      filterCourses(rows, { ...defaultFilters, status: "conflict" }).map((c) => c.code),
    ).toEqual(["GE001"]);
    expect(filterCourses(rows, { ...defaultFilters, sort: "credits" })[0].code).toBe("PR001");
  });
  it("retains viewport, selected node and per-node filters across remounts", () => {
    saveViewState("test-view", {
      viewport: { x: 12, y: 20, zoom: 0.7 },
      expanded: ["plan", "group:general"],
      selected: "group:general",
      filters: { "group:general": { ...defaultFilters, query: "GE", page: 2 } },
    });
    expect(getViewState("test-view").viewport?.zoom).toBe(0.7);
    expect(getViewState("test-view").filters["group:general"].page).toBe(2);
    expect(getViewState("other-view").selected).toBeUndefined();
  });
});

it("uses the reported completion flag rather than inferring passage from credits", () => {
  expect(completionSummary({ YQXF: 10, WCXF: 10, SFTG: "0" })).toContain("未通过");
  expect(completionSummary({ YQXF: 10, WCXF: 10 })).toContain("状态未知");
  expect(completionSummary({ SFTG: "1" })).toContain("完成学分未知");
});

it("keeps same-name real groups separate and migrates old synthetic nodes", () => {
  const tree = buildTree({
    ...academicFixture,
    modules: [
      { module: { KZH: "a", KZM: "专业课" }, courses: [{ KCH: "C", KCLBDM_DISPLAY: "专业课" }] },
      { module: { KZH: "b", FKZH: "a", KZM: "专业课" }, courses: [{ KCH: "D" }] },
    ],
  });
  expect(tree.nodes.size).toBe(3);
  expect(tree.nodes.get("group:a")!.children).toEqual(["group:b"]);
  expect(aggregateCourses(tree, "group:a")).toHaveLength(2);
  expect(courseCategory({ KCXZDM_DISPLAY: "必修" }).label).toBe("类型未提供");
  saveViewState("legacy", {
    selected: "category:group:a:code%3A1",
    expanded: ["plan", "course:group:b:D"],
    filters: { "category:group:a:code%3A1": { ...defaultFilters, query: "C" } },
  });
  const migrated = migrateViewState("legacy", tree);
  expect(migrated.selected).toBe("group:a");
  expect(migrated.expanded).toContain("group:b");
  expect(migrated.filters["group:a"].query).toBe("C");
});
