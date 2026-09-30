import { useState } from "react";
import { Button, CloseButton, Table } from "@heroui/react";
import { LuArrowLeft, LuArrowUpRight } from "react-icons/lu";
import { PageShell } from "./components";
import { useLogicalNavigation } from "./logical-navigation";
import { openPublicUrl } from "./app-content/external";

const sourceUrl = "https://jwb.szu.edu.cn/bszn1/sksjb.htm";

// Transcribed from the teaching affairs office image, checked on 2026-09-29.
// Keep the two evening blocks intact: the source does not give individual times.
const rows = [
  { id: "lesson-1", period: "上午", session: "第一节", time: "8:30 — 9:10" },
  { id: "lesson-2", period: "上午", session: "第二节", time: "9:15 — 9:55" },
  { id: "morning-break", period: "上午", session: "大课间", time: "20 分钟" },
  { id: "lesson-3", period: "上午", session: "第三节", time: "10:15 — 10:55" },
  { id: "lesson-4", period: "上午", session: "第四节", time: "11:00 — 11:40" },
  { id: "lesson-5", period: "上午", session: "第五节", time: "11:45 — 12:25" },
  { id: "lesson-6", period: "下午", session: "第六节", time: "13:30 — 14:10" },
  { id: "lesson-7", period: "下午", session: "第七节", time: "14:15 — 14:55" },
  { id: "lesson-8", period: "下午", session: "第八节", time: "15:00 — 15:40" },
  { id: "afternoon-break", period: "下午", session: "大课间", time: "20 分钟" },
  { id: "lesson-9", period: "下午", session: "第九节", time: "16:00 — 16:40" },
  { id: "lesson-10", period: "下午", session: "第十节", time: "16:45 — 17:25" },
  { id: "lessons-11-12", period: "晚上", session: "第十一、十二节", time: "19:00 — 20:20" },
  { id: "lessons-13-14", period: "晚上", session: "第十三、十四节", time: "20:30 — 21:45" },
] as const;

export default function ClassTimetablePage() {
  const navigation = useLogicalNavigation();
  const [openError, setOpenError] = useState<string>();

  return (
    <PageShell>
      <div className="mx-auto grid w-full max-w-3xl min-w-0 gap-6">
        <header className="flex items-center gap-3">
          <CloseButton
            data-webview-navigation
            aria-label="返回工作台"
            onPress={() => void navigation.back()}
          >
            <LuArrowLeft aria-hidden="true" />
          </CloseButton>
          <h1 data-webview-navigation className="text-2xl font-semibold">
            上课时间表
          </h1>
        </header>
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="深圳大学上课时间表">
              <Table.Header>
                <Table.Column id="period">时段</Table.Column>
                <Table.Column id="session" isRowHeader>
                  节次
                </Table.Column>
                <Table.Column id="time">时间</Table.Column>
              </Table.Header>
              <Table.Body>
                {rows.map((row, index) => (
                  <Table.Row
                    key={row.id}
                    id={row.id}
                    className={
                      index > 0 && rows[index - 1].period !== row.period
                        ? "[&>td]:border-t-2 [&>td]:border-t-separator"
                        : undefined
                    }
                  >
                    <Table.Cell className="whitespace-nowrap">{row.period}</Table.Cell>
                    <Table.Cell className="whitespace-nowrap">{row.session}</Table.Cell>
                    <Table.Cell className="whitespace-nowrap tabular-nums">{row.time}</Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
        <footer className="grid justify-items-start gap-2">
          <Button
            size="sm"
            variant="tertiary"
            onPress={() => {
              setOpenError(undefined);
              void openPublicUrl(sourceUrl).catch(() =>
                setOpenError("无法打开系统浏览器，请稍后重试。"),
              );
            }}
          >
            来源：深圳大学教务部
            <LuArrowUpRight aria-hidden="true" />
          </Button>
          {openError && (
            <p role="alert" className="content-error">
              {openError}
            </p>
          )}
        </footer>
      </div>
    </PageShell>
  );
}
