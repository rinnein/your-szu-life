import { useId, useRef, useState, useEffect } from "react";
import { Button, Label, TextArea, toast } from "@heroui/react";
import { LuCopy, LuSpace } from "react-icons/lu";
import { copyText } from "../clipboard";
import { notifySuccess } from "./feedback";

export function WhitespaceOverlay({ text }: { text: string }) {
  const tokens = text.match(/([ ]+|\t|\r?\n|[^\t\r\n ]+)/g) ?? [];
  return (
    <>
      {tokens.map((token, i) => {
        if (token.startsWith(" ")) {
          return (
            <span
              key={i}
              className="text-muted/50 dark:text-muted/60 select-none"
              data-whitespace="space"
            >
              {"·".repeat(token.length)}
            </span>
          );
        }
        if (token === "\t") {
          return (
            <span
              key={i}
              className="relative inline-block text-muted/50 dark:text-muted/60 select-none"
              data-whitespace="tab"
            >
              <span className="absolute inset-0 pointer-events-none select-none overflow-hidden">
                →
              </span>
              {"\t"}
            </span>
          );
        }
        if (token === "\n" || token === "\r\n") {
          return token;
        }
        return (
          <span key={i} className="text-transparent select-none">
            {token}
          </span>
        );
      })}
    </>
  );
}

export function SampleField({
  label,
  value,
  sample,
}: {
  label: string;
  value: string;
  sample: number;
}) {
  const id = useId();
  const element = useRef<HTMLTextAreaElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);
  const [showSpaces, setShowSpaces] = useState(false);

  useEffect(() => {
    if (showSpaces && element.current && overlayRef.current) {
      overlayRef.current.scrollTop = element.current.scrollTop;
      overlayRef.current.scrollLeft = element.current.scrollLeft;
    }
  }, [showSpaces]);

  const handleScroll = () => {
    if (element.current && overlayRef.current) {
      overlayRef.current.scrollTop = element.current.scrollTop;
      overlayRef.current.scrollLeft = element.current.scrollLeft;
    }
  };

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <Label htmlFor={id}>{label}</Label>
        <div className="flex items-center gap-1">
          <Button
            size="sm"
            variant={showSpaces ? "secondary" : "tertiary"}
            aria-label={`${showSpaces ? "隐藏" : "显示"}样例 ${sample} ${label}空格`}
            onPress={() => setShowSpaces((prev) => !prev)}
          >
            <LuSpace aria-hidden="true" />
            {showSpaces ? "隐藏空格" : "显示空格"}
          </Button>
          <Button
            size="sm"
            variant="tertiary"
            aria-label={`复制样例 ${sample} ${label}`}
            onPress={() => {
              // Copy the original string, not textarea.value (which normalizes CRLF).
              void copyText(value).then((copied) => {
                if (copied) notifySuccess(`样例 ${sample} ${label}已复制。`);
                else {
                  element.current?.focus();
                  element.current?.select();
                  toast.warning("自动复制失败，已选中样例，请手动复制。", { timeout: 3000 });
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
        <TextArea
          ref={element}
          id={id}
          aria-label={`样例 ${sample} ${label}`}
          readOnly
          value={value}
          rows={Math.max(3, Math.min(12, value.split("\n").length))}
          wrap="off"
          spellCheck={false}
          fullWidth
          className="resize-y font-mono whitespace-pre text-sm leading-6"
          onScroll={handleScroll}
        />
        {showSpaces && (
          <div
            ref={overlayRef}
            aria-hidden="true"
            data-testid="whitespace-overlay"
            className="pointer-events-none select-none absolute inset-0 overflow-hidden font-mono whitespace-pre text-sm leading-6 px-3 py-2 border border-transparent"
          >
            <WhitespaceOverlay text={value} />
          </div>
        )}
      </div>
    </div>
  );
}
