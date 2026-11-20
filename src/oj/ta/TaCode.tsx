import { lazy, Suspense } from "react";
import { LoadingIndicator } from "material/react";
import { CodeEditor } from "../CodeEditor";

const MonacoEditor = lazy(() => import("../MonacoEditor"));
const unchanged = () => {};

/** 助教页面的只读代码块；桌面用 Monaco，移动端沿用 CodeMirror。 */
export function TaCode({
  value,
  language,
  desktop,
  ariaLabel,
}: {
  value: string;
  language?: string | null;
  desktop: boolean;
  ariaLabel: string;
}) {
  if (!desktop) {
    return (
      <CodeEditor
        value={value}
        language={language ?? ""}
        disabled
        onChange={unchanged}
        ariaLabel={ariaLabel}
      />
    );
  }
  return (
    <Suspense fallback={<LoadingIndicator size={24} aria-label="加载只读编辑器" />}>
      <MonacoEditor
        value={value}
        language={language ?? ""}
        disabled
        onChange={unchanged}
        ariaLabel={ariaLabel}
      />
    </Suspense>
  );
}
