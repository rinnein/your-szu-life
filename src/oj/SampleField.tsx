import { Fragment, useId, useMemo, useRef } from "react";
import { Button } from "material/react";
import { toast } from "../ui";
import { LuCopy, LuSpace } from "react-icons/lu";
import { copyText } from "../clipboard";
import { notifySuccess } from "./feedback";
import { OJ_MONO_FONT_FAMILY } from "./monospace";
import {
  useWhitespaceVisible,
  toggleWhitespaceVisible,
  whitespaceStore,
  setWhitespaceVisible,
  resetWhitespaceVisible,
} from "./whitespace-store";

export {
  whitespaceStore,
  useWhitespaceVisible,
  toggleWhitespaceVisible,
  setWhitespaceVisible,
  resetWhitespaceVisible,
};

/** 文本层与标记层共用的几何常量；容器高度与内容宽度由它们推导。 */
const LINE_HEIGHT = 24;
const PAD_X = 16;
const PAD_Y = 12;
/** 浏览器原生制表位宽度（与两侧显式钉住的 tabSize 一致）。 */
const TAB_SIZE = 8;

type Token =
  | { kind: "text"; value: string }
  | { kind: "space"; count: number }
  | { kind: "tab"; width: number };

/**
 * 把一行文本展开为列位置感知的 token 序列（Monaco renderWhitespace 式）：
 * tab 的 advance 取决于所在列（(tabSize - col % tabSize) 列），空格串按字符数计列，
 * 因此标记层可以用 `ch` 单位给出与文本层完全一致的宽度，不依赖任何字形度量。
 */
function layoutLine(line: string): Token[] {
  const tokens: Token[] = [];
  let col = 0;
  for (const m of line.matchAll(/([ ]+|\t|[^\t ]+)/g)) {
    const t = m[0];
    if (t.startsWith(" ")) {
      tokens.push({ kind: "space", count: t.length });
      col += t.length;
    } else if (t === "\t") {
      const width = TAB_SIZE - (col % TAB_SIZE);
      tokens.push({ kind: "tab", width });
      col += width;
    } else {
      tokens.push({ kind: "text", value: t });
      col += t.length;
    }
  }
  return tokens;
}

const tokenCols = (tokens: Token[]) =>
  tokens.reduce(
    (c, t) => c + (t.kind === "text" ? t.value.length : t.kind === "space" ? t.count : t.width),
    0,
  );

function MarkerToken({ token }: { token: Token }) {
  if (token.kind === "space" || token.kind === "tab") {
    return (
      <span
        className="text-muted/50 dark:text-muted/60 select-none"
        data-whitespace={token.kind}
        style={{
          display: "inline-block",
          width: `${token.kind === "space" ? token.count : token.width}ch`,
          // overflow:hidden 的 inline-block 基线取底边，会把行盒撑高；
          // 高度等于行高（line-height 继承），顶对齐后字形与文本同带。
          verticalAlign: "top",
          overflow: "hidden",
          whiteSpace: "pre",
        }}
      >
        {token.kind === "space" ? "·".repeat(token.count) : "→"}
      </span>
    );
  }
  return <span className="text-transparent select-none">{token.value}</span>;
}

export interface SampleFieldProps {
  label: string;
  value?: string;
  sample?: number | string;
  ariaLabel?: string;
  placeholder?: string;
  minRows?: number;
  maxRows?: number;
}

export function SampleField({
  label,
  value,
  sample,
  ariaLabel,
  placeholder,
  minRows = 3,
  maxRows = 12,
}: SampleFieldProps) {
  const id = useId();
  const element = useRef<HTMLTextAreaElement>(null);
  const showSpaces = useWhitespaceVisible();

  const itemLabel = ariaLabel ?? (sample !== undefined ? `样例 ${sample} ${label}` : label);
  const hasValue = value !== undefined && value !== "";

  const lines = useMemo(
    () => (hasValue && value !== undefined ? value.replace(/\r\n?/g, "\n").split("\n") : []),
    [hasValue, value],
  );
  const lineTokens = useMemo(() => lines.map(layoutLine), [lines]);
  // 内容 wrapper 按“最宽行的展开列数”撑开，双向滚动都发生在外层容器上，
  // 文本层与标记层随之同流滚动，无需任何滚动位置复制。
  const maxCols = useMemo(
    () => Math.max(1, ...lineTokens.map((tokens) => tokenCols(tokens))),
    [lineTokens],
  );
  const clampedRows = Math.max(minRows, Math.min(maxRows, lines.length || minRows));

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <label className="font-medium text-sm" htmlFor={id}>
          {label}
        </label>
        <div className="flex items-center gap-1">
          <Button
            size="xs"
            variant={showSpaces ? "tonal" : "text"}
            aria-label={`${showSpaces ? "隐藏" : "显示"}${itemLabel}空格`}
            disabled={!hasValue}
            onClick={toggleWhitespaceVisible}
          >
            <LuSpace aria-hidden="true" />
            {showSpaces ? "隐藏空格" : "显示空格"}
          </Button>
          <Button
            size="xs"
            variant="text"
            aria-label={`复制${itemLabel}`}
            disabled={!hasValue}
            onClick={() => {
              if (!hasValue || value === undefined) return;
              // Copy the original string, not textarea.value (which normalizes CRLF).
              void copyText(value).then((copied) => {
                if (copied) notifySuccess(`${itemLabel}已复制。`);
                else {
                  element.current?.focus();
                  element.current?.select();
                  toast.warning("自动复制失败，已选中文本，请手动复制。", { timeout: 3000 });
                }
              });
            }}
          >
            <LuCopy aria-hidden="true" />
            复制
          </Button>
        </div>
      </div>
      <div className="relative min-w-0">
        {hasValue ? (
          <div
            className="md-sample-field overflow-auto resize-y"
            style={{
              height: `${clampedRows * LINE_HEIGHT + PAD_Y * 2 + 2}px`,
            }}
            tabIndex={0}
          >
            <div
              className="relative"
              style={{
                minWidth: "100%",
                width: `calc(${maxCols}ch + ${PAD_X * 2}px)`,
                fontFamily: OJ_MONO_FONT_FAMILY,
                fontSize: "14px",
                lineHeight: `${LINE_HEIGHT}px`,
              }}
            >
              <textarea
                ref={element}
                id={id}
                aria-label={itemLabel}
                readOnly
                value={value}
                rows={lines.length}
                wrap="off"
                spellCheck={false}
                className="whitespace-pre"
                style={{
                  display: "block",
                  overflow: "hidden",
                  border: "none",
                  outline: "none",
                  resize: "none",
                  background: "transparent",
                  color: "inherit",
                  width: "100%",
                  padding: `${PAD_Y}px ${PAD_X}px`,
                  tabSize: TAB_SIZE,
                  fontFamily: "inherit",
                  fontSize: "14px",
                  lineHeight: `${LINE_HEIGHT}px`,
                }}
              />
              {showSpaces && (
                <div
                  aria-hidden="true"
                  data-testid="whitespace-overlay"
                  className="pointer-events-none absolute inset-0 overflow-hidden whitespace-pre select-none"
                  style={{
                    padding: `${PAD_Y}px ${PAD_X}px`,
                    tabSize: TAB_SIZE,
                    boxSizing: "border-box",
                  }}
                >
                  {lineTokens.map((tokens, line) => (
                    <Fragment key={line}>
                      {tokens.map((token, i) => (
                        <MarkerToken key={i} token={token} />
                      ))}
                      {line < lineTokens.length - 1 && "\n"}
                    </Fragment>
                  ))}
                </div>
              )}
            </div>
          </div>
        ) : (
          <p className="text-xs text-muted italic py-1">{placeholder ?? "（未提供）"}</p>
        )}
      </div>
    </div>
  );
}
