import { Fragment } from "react";
import { DrawerItem } from "material/react";
import { problemStatus, statusPillHtml } from "./problem-status";
import { kindName, problemKey, type Problem } from "./types";

/**
 * 题目列表的声明行，宽屏常开抽屉与窄屏模态抽屉共用。
 *
 * material 的 `<m-drawer-item>` 是声明元素：自己不渲染任何内容，只被外层 Drawer 读成
 * 配置、再在 shadow root 里建行。所以这里产出的是声明而不是可视 DOM，判定胶囊也因此
 * 只能走 `icon` 属性 —— 它是唯一接受标记的槽位（`badge` 只收纯文本）。
 */
export function ProblemList({
  problems,
  verdict,
  emptyVerdict,
}: {
  problems: Problem[];
  verdict: (problem: Problem) => number | undefined;
  emptyVerdict: (problem: Problem) => string;
}) {
  return (
    <>
      {[0, 1, 2, 3, 4]
        .filter((kind) => problems.some((p) => p.kind === kind))
        .map((kind) => {
          const group = problems.filter((p) => p.kind === kind);
          const accepted = group.filter((p) => verdict(p) === 0).length;
          return (
            <Fragment key={kind}>
              <DrawerItem
                type="section"
                // 编程题与代码补全会等待服务端判定，进度是它们的主要指标。
                label={
                  [0, 4].includes(kind)
                    ? `${kindName[kind]} ${accepted}/${group.length}`
                    : kindName[kind]
                }
              />
              {group.map((problem) => {
                const status = problemStatus(problem, verdict, emptyVerdict);
                return (
                  <DrawerItem
                    key={problemKey(problem)}
                    value={problemKey(problem)}
                    label={`${problem.displayLabel}. ${problem.title}`}
                    icon={statusPillHtml(status.tone, status.text)}
                  />
                );
              })}
            </Fragment>
          );
        })}
    </>
  );
}
