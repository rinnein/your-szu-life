import { useState } from "react";
import { Button } from "material/react";
import { Table } from "../ui";
import { LuArrowUpRight } from "react-icons/lu";
import { openPublicUrl } from "../app-content/external";
import { PERIOD_ROWS } from "./periods";

const SOURCE_URL = "https://jwb.szu.edu.cn/bszn1/sksjb.htm";

/** 深圳大学标准作息表；原「上课时间表」独立页迁入课表模块。 */
export function PeriodTable() {
  const [openError, setOpenError] = useState<string>();

  return (
    <div className="grid gap-6">
      <Table className="shadow-sm">
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
              {PERIOD_ROWS.map((row, index) => (
                <Table.Row
                  key={row.id}
                  id={row.id}
                  className={
                    index > 0 && PERIOD_ROWS[index - 1].period !== row.period
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
          size="xs"
          variant="text"
          onClick={() => {
            setOpenError(undefined);
            void openPublicUrl(SOURCE_URL).catch(() =>
              setOpenError("无法打开系统浏览器，请稍后重试。"),
            );
          }}
        >
          来源：深圳大学教务部
          <LuArrowUpRight aria-hidden="true" />
        </Button>
        {openError && (
          <p role="alert" className="text-sm text-red-600">
            {openError}
          </p>
        )}
      </footer>
    </div>
  );
}
