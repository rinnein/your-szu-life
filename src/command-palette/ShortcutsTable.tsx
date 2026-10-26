import { Card } from "material/react";
import { Chip, Kbd, Table } from "../ui";

export interface ShortcutEntry {
  shortcut: string;
  name: string;
  description: string;
  target: string;
}

export const SHORTCUT_ENTRIES: ShortcutEntry[] = [
  {
    shortcut: "time",
    name: "上课时间表",
    description: "查看各节次上课时间与作息",
    target: "/class-timetable",
  },
  {
    shortcut: "oj",
    name: "OJ首页",
    description: "在线评测系统首页与题库",
    target: "/oj",
  },
  {
    shortcut: "ojzj",
    name: "最近班级测验",
    description: "最后一次打开的OJ的最后一次打开的班级的测验列表",
    target: "/oj?site=...&group=...",
  },
  {
    shortcut: "uooc / mooc",
    name: "UOOC任务tab",
    description: "优课在线平时视频任务",
    target: "/uooc?tab=tasks",
  },
  {
    shortcut: "net / xyw",
    name: "校园网登录页面",
    description: "教工区与宿舍区网络认证",
    target: "/campus-network",
  },
  {
    shortcut: "card / xyk",
    name: "校园卡tab",
    description: "校园卡充值与服务",
    target: "/campus-payments/card",
  },
  {
    shortcut: "cdl",
    name: "查电量tab",
    description: "宿舍电量剩余与用电明细",
    target: "/campus-payments/electricity",
  },
  {
    shortcut: "cdf",
    name: "充电费tab",
    description: "官方微信服务宿舍电费充值",
    target: "/campus-payments/recharge",
  },
];

export function ShortcutsTable() {
  return (
    <Card aria-labelledby="shortcuts-table-title" variant="outlined">
      <div slot="headline" className="flex flex-wrap items-center justify-between gap-3 w-full">
        <span id="shortcuts-table-title">命令面板快捷指令对照表</span>
        <div className="flex items-center gap-1.5 text-xs text-muted">
          <span>全局快捷键：</span>
          <Kbd>⌘ ⇧ P</Kbd>
        </div>
      </div>
      <div slot="subhead">
        在应用内任意页面按下快捷键唤起命令面板，输入下方英文缩写或功能名称即可快速跳转。
      </div>
      <div className="pt-2 w-full min-w-0 max-w-full overflow-hidden">
        <p className="text-[11px] text-muted mb-2 sm:hidden">
          左右滑动表格可查看完整说明与跳转目标
        </p>
        <Table aria-label="快捷键与缩写对照表">
          <Table.ScrollContainer className="w-full min-w-0 max-w-full overflow-x-auto">
            <Table.Content aria-label="快捷键对照" className="min-w-[480px]">
              <Table.Header>
                <Table.Column isRowHeader>英文缩写 / 快捷指令</Table.Column>
                <Table.Column>功能名称</Table.Column>
                <Table.Column>功能说明与跳转目标</Table.Column>
              </Table.Header>
              <Table.Body>
                {SHORTCUT_ENTRIES.map((entry) => (
                  <Table.Row key={entry.shortcut} id={entry.shortcut}>
                    <Table.Cell className="font-mono whitespace-nowrap">
                      <div className="flex items-center gap-1.5">
                        {entry.shortcut.split(" / ").map((s) => (
                          <Chip key={s} size="sm" variant="soft" color="accent">
                            {s}
                          </Chip>
                        ))}
                      </div>
                    </Table.Cell>
                    <Table.Cell className="font-medium whitespace-nowrap">{entry.name}</Table.Cell>
                    <Table.Cell className="text-muted">
                      <div>{entry.description}</div>
                      <div className="text-xs font-mono text-muted/80">{entry.target}</div>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      </div>
    </Card>
  );
}
