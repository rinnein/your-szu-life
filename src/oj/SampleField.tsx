import { useId, useRef } from "react";
import { Button, Label, TextArea, toast } from "@heroui/react";
import { LuCopy } from "react-icons/lu";
import { copyText } from "../clipboard";
import { notifySuccess } from "./feedback";

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
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <Label htmlFor={id}>{label}</Label>
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
        className="resize-y font-mono whitespace-pre"
      />
    </div>
  );
}
