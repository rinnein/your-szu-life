import type { Viewport } from "@xyflow/react";
import { owningGroup, type AcademicTree, type Filters } from "./tree";
export type ViewState = {
  expanded: string[];
  viewport?: Viewport;
  selected?: string;
  filters: Record<string, Filters>;
};
const states = new Map<string, ViewState>();
export function getViewState(key: string): ViewState {
  let value = states.get(key);
  if (!value) {
    value = { expanded: ["plan"], filters: {} };
    states.set(key, value);
  }
  return value;
}
export function saveViewState(key: string, patch: Partial<ViewState>) {
  const next = { ...getViewState(key), ...patch };
  states.set(key, next);
  return next;
}

export function migrateViewState(key: string, tree: AcademicTree) {
  const state = getViewState(key);
  const filters = { ...state.filters };
  for (const [id, value] of Object.entries(state.filters)) {
    const owner = owningGroup(tree, id);
    if (owner && !filters[owner]) filters[owner] = value;
  }
  return saveViewState(key, {
    expanded: [
      ...new Set(
        state.expanded.map((id) => owningGroup(tree, id)).filter((id): id is string => !!id),
      ),
    ],
    selected: owningGroup(tree, state.selected),
    filters,
  });
}
