import {
  Fragment,
  createContext,
  useContext,
  type ReactNode,
  type TableHTMLAttributes,
  type TdHTMLAttributes,
  type ThHTMLAttributes,
} from "react";

/**
 * 表内共享的元信息。
 *
 * 把某一列标为 `isRowHeader` 后，该列在**正文行**里的单元格也会
 * 变成行标题（`<th scope="row">`）。表头先渲染，这里借一个可写的序号集合
 * 把列信息传给后面的正文单元格。
 */
const TableMetaContext = createContext<{
  rowHeaderColumns: Set<number>;
  columnCursor: { current: number };
  cellCursor: { current: number };
} | null>(null);

/**
 * 多列数据表格：material 没有对应组件，自建。
 *
 * material 只有 List（单列索引），没有多列数据表格，因此自建真 `<table>`。
 * 保留复合 API 形状（`Table.ScrollContainer` / `Table.Content` /
 * `Table.Header` / `Table.Column` / `Table.Body` / `Table.Row` / `Table.Cell`），
 * 使用点只需换导入。
 *
 * 两点设计保证无障碍与测试可查询：
 * - `role="grid"`：读屏与既有测试都按这个角色查询。
 * - `Header` 自动补 `<tr>`：调用点写 `<Table.Header><Table.Column>`，
 *   而 HTML 要求 `<th>` 必须在 `<tr>` 里。
 *
 * 现有用法都是静态表格（无排序、无虚拟滚动、无列宽调整），
 * 因此不引入 TanStack Table —— 那是纯负担。
 */

function ScrollContainer({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={`data-table-scroll ${className ?? ""}`}>{children}</div>;
}

function Content({
  children,
  className,
  "aria-label": ariaLabel,
  ...rest
}: TableHTMLAttributes<HTMLTableElement>) {
  // 每次渲染重建，避免上一张表的列标记残留。
  const meta = {
    rowHeaderColumns: new Set<number>(),
    columnCursor: { current: 0 },
    cellCursor: { current: 0 },
  };
  return (
    <TableMetaContext.Provider value={meta}>
      <table
        className={`data-table ${className ?? ""}`}
        role="grid"
        aria-label={ariaLabel}
        {...rest}
      >
        {children}
      </table>
    </TableMetaContext.Provider>
  );
}

function Header({
  children,
  className,
  columns,
  renderEmptyState: _renderEmptyState,
}: {
  children?: ReactNode;
  className?: string;
  /** 只支持子元素写法。 */
  columns?: unknown;
  renderEmptyState?: () => ReactNode;
}) {
  void columns;
  void _renderEmptyState;
  return (
    <thead className={className}>
      <tr>{children}</tr>
    </thead>
  );
}

function Column({
  children,
  className,
  id,
  isRowHeader,
  ...rest
}: ThHTMLAttributes<HTMLTableCellElement> & {
  /** 表头同时作为该行的行标题时用 `scope="row"`，角色才是 rowheader。 */
  isRowHeader?: boolean;
  id?: string;
}) {
  const meta = useContext(TableMetaContext);
  const index = meta ? meta.columnCursor.current++ : -1;
  // 记录哪些列在**正文行**里要成为行标题；表头本身仍是列标题。
  if (meta && isRowHeader) meta.rowHeaderColumns.add(index);

  return (
    <th
      // 显式声明角色：jsdom 不按 scope 推导行列标题，
      // 只写 scope 会让 getByRole("columnheader") 查不到。
      role="columnheader"
      scope="col"
      id={id}
      className={className}
      {...rest}
    >
      {children}
    </th>
  );
}

function Body<T>({
  children,
  className,
  items,
  renderEmptyState,
}: {
  children: ReactNode | ((item: T) => ReactNode);
  className?: string;
  /** 也支持 items + 渲染函数生成行。 */
  items?: readonly T[];
  renderEmptyState?: () => ReactNode;
}) {
  const list = items ?? [];
  const rows =
    typeof children === "function"
      ? list.map((item, index) => <Fragment key={index}>{children(item)}</Fragment>)
      : children;

  // 空表给一个占位行，否则 role="grid" 的表格没有行会被读屏忽略。
  const empty =
    typeof children === "function" && list.length === 0 && renderEmptyState
      ? renderEmptyState()
      : null;

  return <tbody className={className}>{empty ?? rows}</tbody>;
}

function Row({
  children,
  className,
  onClick,
  id: _id,
  key: _key,
  "aria-selected": ariaSelected,
}: {
  children: ReactNode;
  className?: string;
  onClick?: () => void;
  /** 原生表格用 key 标识行。 */
  id?: string | number;
  key?: string | number;
  "aria-selected"?: boolean;
}) {
  void _id;
  void _key;
  // 每行开始时把单元格序号归零，供 Cell 判断自己是不是行标题列。
  const meta = useContext(TableMetaContext);
  if (meta) meta.cellCursor.current = 0;
  return (
    <tr
      className={className}
      onClick={onClick}
      aria-selected={ariaSelected}
      // 可点击行必须可聚焦，否则键盘用户无法触发。
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={
        onClick
          ? (event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onClick();
              }
            }
          : undefined
      }
    >
      {children}
    </tr>
  );
}

function Cell({
  children,
  className,
  colSpan,
  isRowHeader,
  ...rest
}: TdHTMLAttributes<HTMLTableCellElement> & { isRowHeader?: boolean }) {
  const meta = useContext(TableMetaContext);
  const index = meta ? meta.cellCursor.current++ : -1;
  // 表头标了 isRowHeader 的那一列，在正文行里也要成为行标题。
  const asRowHeader = isRowHeader ?? (meta ? meta.rowHeaderColumns.has(index) : false);

  return asRowHeader ? (
    <th
      role="rowheader"
      scope="row"
      className={className}
      colSpan={colSpan}
      {...(rest as ThHTMLAttributes<HTMLTableCellElement>)}
    >
      {children}
    </th>
  ) : (
    <td className={className} colSpan={colSpan} {...rest}>
      {children}
    </td>
  );
}

/** 根容器：允许 `<Table className>` 包住 ScrollContainer。 */
const TableRoot = ({ children, className }: { children: ReactNode; className?: string }) => (
  <div className={`md-table-root ${className ?? ""}`}>{children}</div>
);

export const Table = Object.assign(TableRoot, {
  ScrollContainer,
  Content,
  Header,
  Column,
  Body,
  Row,
  Cell,
});
