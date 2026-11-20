import { route } from "../types";

export type TaView = "students" | "problems";
export type TaTab = "students" | "statement" | "judge" | "answer";

export function taWorkbenchRoute(site: string) {
  return route({ site, view: "ta" });
}

export function taContestRoute(site: string, contest: string, taView: TaView = "students") {
  return route({ site, view: "ta", contest, taView });
}

export function taReviewRoute(site: string, contest: string, review: string) {
  return route({
    site,
    view: "ta",
    contest,
    taView: "students",
    review,
  });
}

export function taProblemRoute(
  site: string,
  contest: string,
  problem?: string,
  taTab?: TaTab,
  codeSubmission?: string,
) {
  return route({
    site,
    view: "ta",
    contest,
    taView: "problems",
    problem,
    taTab: problem ? taTab : undefined,
    codeSubmission: taTab === "judge" ? codeSubmission : undefined,
  });
}

export function studentContestRoute(
  site: string,
  contest: string,
  problem?: string,
  tab: "statement" | "answer" | "records" = "statement",
) {
  return route({ site, view: "student", contest, problem, tab });
}
