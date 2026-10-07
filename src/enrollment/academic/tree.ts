import type { Academic } from "../model";
export type RecordRow = Record<string, unknown>;
export type TreeNode = {
  id: string;
  parent: string | null;
  kind: "plan" | "group";
  label: string;
  children: string[];
  row: RecordRow;
  group?: string;
  warning?: string;
};
export type AcademicTree = {
  root: string;
  nodes: Map<string, TreeNode>;
  courses: Map<string, RecordRow[]>;
  warnings: string[];
};
export type CourseResult = {
  id: string;
  code: string;
  names: string[];
  credits: string[];
  types: string[];
  statuses: string[];
  groups: { id: string; label: string }[];
  records: RecordRow[];
  conflict: boolean;
};
export const string = (v: unknown) => (v == null ? "" : String(v));
export function number(v: unknown): number | null {
  if (v == null || v === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}
export function status(row: RecordRow) {
  return string(row.SFTG) === "1"
    ? "已完成"
    : string(row.SFXK) === "1"
      ? "已选未完成"
      : string(row.SFTG) === "2"
        ? "未修"
        : "未知";
}
export function creditSummary(row: RecordRow) {
  const required = number(row.YQXF),
    done = number(row.WCXF);
  return required === null || done === null ? "完成学分未知" : `${done} / ${required} 学分`;
}
export function completionSummary(row: RecordRow) {
  const passed = string(row.SFTG);
  const label = ["1", "true", "是"].includes(passed)
    ? "已通过"
    : ["0", "false", "否"].includes(passed)
      ? "未通过"
      : "状态未知";
  return `${label} · ${creditSummary(row)}`;
}
export function courseCategory(row: RecordRow) {
  const code = string(row.KCLBDM).trim();
  const name = string(row.KCLBDM_DISPLAY).trim();
  return {
    key: code ? `code:${code}` : name ? `name:${name}` : "missing",
    label: name || (code ? `未命名类别（${code}）` : "类型未提供"),
  };
}
export function buildTree(academic: Academic): AcademicTree {
  const root = "plan";
  const nodes = new Map<string, TreeNode>();
  const warnings: string[] = [];
  nodes.set(root, {
    id: root,
    parent: null,
    kind: "plan",
    label: string(academic.summary[0]?.PYFAMC) || "当前培养方案",
    row: academic.summary[0] ?? {},
    children: [],
  });
  const codes = new Map<string, string>();
  academic.modules.forEach((entry, i) => {
    const code = string(entry.module.KZH);
    const id = `group:${code || `missing-${i}`}`;
    if (nodes.has(id)) {
      warnings.push("存在重复课组记录，已保留课程明细");
      return;
    }
    if (code) codes.set(code, id);
    nodes.set(id, {
      id,
      parent: root,
      kind: "group",
      label: string(entry.module.KZM) || "未命名课组",
      row: entry.module,
      children: [],
    });
  });
  for (const node of nodes.values())
    if (node.kind === "group") {
      const parent = string(node.row.FKZH);
      node.parent = parent ? (codes.get(parent) ?? root) : root;
      if (parent && !codes.has(parent) && parent !== "0" && parent !== "-1") {
        node.warning = "父课组缺失，作为独立分支显示";
        warnings.push(node.warning);
      }
    }
  for (const node of nodes.values())
    if (node.kind === "group") {
      const visited = new Set<string>([node.id]);
      let cursor = node.parent;
      while (cursor && cursor !== root) {
        if (visited.has(cursor)) {
          node.parent = root;
          node.warning = "检测到循环关系，已断开循环";
          warnings.push(node.warning);
          break;
        }
        visited.add(cursor);
        cursor = nodes.get(cursor)?.parent ?? null;
      }
    }
  const courses = new Map<string, RecordRow[]>();
  academic.modules.forEach((entry, i) => {
    const group = `group:${string(entry.module.KZH) || `missing-${i}`}`;
    courses.set(group, [...(courses.get(group) ?? []), ...entry.courses]);
  });
  for (const node of nodes.values())
    if (node.parent) nodes.get(node.parent)?.children.push(node.id);
  return { root, nodes, courses, warnings: [...new Set(warnings)] };
}
export function descendants(tree: AcademicTree, id: string): TreeNode[] {
  const result: TreeNode[] = [];
  const visited = new Set<string>();
  const visit = (id: string) => {
    if (visited.has(id)) return;
    visited.add(id);
    const node = tree.nodes.get(id);
    if (!node) return;
    result.push(node);
    node.children.forEach(visit);
  };
  visit(id);
  return result;
}
export function nodePath(tree: AcademicTree, id: string) {
  const path: string[] = [];
  const visited = new Set<string>();
  let node = tree.nodes.get(id);
  while (node && !visited.has(node.id)) {
    visited.add(node.id);
    path.unshift(node.label);
    node = node.parent ? tree.nodes.get(node.parent) : undefined;
  }
  return path;
}
export function aggregateCourses(tree: AcademicTree, id: string): CourseResult[] {
  const result = new Map<string, CourseResult>();
  for (const group of descendants(tree, id))
    for (const [index, row] of (tree.courses.get(group.id) ?? []).entries()) {
      const node = {
        id: `${group.id}:record:${index}`,
        row,
        label: string(row.KCM) || string(row.KCH) || "未知课程",
        group: group.id,
      };
      const code = string(node.row.KCH);
      const key = code || node.id;
      let value = result.get(key);
      if (!value) {
        value = {
          id: key,
          code,
          names: [],
          credits: [],
          types: [],
          statuses: [],
          groups: [],
          records: [],
          conflict: false,
        };
        result.set(key, value);
      }
      const add = (list: string[], v: string) => {
        if (!list.includes(v)) list.push(v);
      };
      add(value.names, node.label);
      add(value.credits, number(node.row.XF) === null ? "未知" : String(number(node.row.XF)));
      add(value.types, courseCategory(node.row).label);
      add(value.statuses, status(node.row));
      if (node.group && !value.groups.some((g) => g.id === node.group))
        value.groups.push({
          id: node.group,
          label: tree.nodes.get(node.group)?.label ?? "未知课组",
        });
      value.records.push(node.row);
      value.conflict =
        value.names.length > 1 ||
        value.credits.length > 1 ||
        value.statuses.length > 1 ||
        value.types.length > 1;
    }
  return [...result.values()];
}
export type Filters = { query: string; status: string; group: string; sort: string; page: number };
export const defaultFilters: Filters = {
  query: "",
  status: "all",
  group: "all",
  sort: "code",
  page: 1,
};
export function filterCourses(rows: CourseResult[], filters: Filters) {
  const query = filters.query.trim().toLocaleLowerCase();
  return rows
    .filter(
      (c) =>
        (!query || [c.code, ...c.names].some((s) => s.toLocaleLowerCase().includes(query))) &&
        (filters.status === "all" ||
          (filters.status === "conflict" ? c.conflict : c.statuses.includes(filters.status))) &&
        (filters.group === "all" || c.groups.some((g) => g.id === filters.group)),
    )
    .sort((a, b) =>
      filters.sort === "name"
        ? a.names.join().localeCompare(b.names.join(), "zh-CN")
        : filters.sort === "credits"
          ? (number(b.credits[0]) ?? -1) - (number(a.credits[0]) ?? -1) ||
            a.code.localeCompare(b.code)
          : a.code.localeCompare(b.code),
    );
}

export function owningGroup(tree: AcademicTree, id?: string): string | undefined {
  if (!id) return undefined;
  if (tree.nodes.has(id)) return id;
  return [...tree.nodes.keys()]
    .filter(
      (key) =>
        key.startsWith("group:") &&
        (id.startsWith(`category:${key}:`) || id.startsWith(`course:${key}:`)),
    )
    .sort((a, b) => b.length - a.length)[0];
}
