import { useEffect, useRef, useState } from "react";
import { LoadingIndicator, Button } from "material/react";
import { Banner } from "../ui";
import { init } from "modern-monaco";
import type * as Monaco from "modern-monaco/editor-core";
import { registerOjCompletions } from "./monaco-completions";
import { OJ_MONO_FONT_FAMILY } from "./monospace";
import type { CodeEditorProps } from "./CodeEditor";
let loading: Promise<typeof Monaco> | undefined;
/**
 * Monaco 在 canvas 里按字宽排版，字体没就绪时会用回退字体的度量，
 * 之后换字体就会错位（光标、行号与文本对不齐）。因此初始化前先等字体加载。
 * `document.fonts` 在旧 WebView 上可能缺失，缺失时直接放行。
 */
function fontsReady() {
  const fonts = typeof document !== "undefined" ? document.fonts : undefined;
  if (!fonts) return Promise.resolve();
  return Promise.all([
    fonts.load("14px 'Monaspace Neon'"),
    // 等待已声明字体的加载结束，避免逐次 load 的竞态。
    fonts.ready,
  ]).then(
    () => undefined,
    () => undefined,
  );
}
function loadMonaco() {
  return (loading ??= fontsReady()
    .then(() =>
      init({
        cdn: window.location.origin,
        defaultTheme: "vitesse-light",
        themes: ["vitesse-dark"],
        langs: ["c", "cpp", "java", "python", "go"],
      }),
    )
    .then((monaco) => {
      registerOjCompletions(monaco);
      return monaco;
    })
    .catch((error) => {
      loading = undefined;
      throw error;
    }));
}
const languageId = (language: string) =>
  ({ c: "c", cpp: "cpp", java: "java", py: "python", go: "go", js: "javascript" })[language] ??
  "plaintext";
export default function MonacoEditor(props: CodeEditorProps) {
  const { value, language, disabled, ariaLabel = "代码编辑器" } = props;
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<Monaco.editor.IStandaloneCodeEditor>(undefined);
  const model = useRef<Monaco.editor.ITextModel>(undefined);
  const latest = useRef(props);
  latest.current = props;
  const external = useRef(false);
  const [error, setError] = useState<string>();
  const [ready, setReady] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let disposed = false;
    let release: (() => void) | undefined;
    setError(undefined);
    setReady(false);
    void loadMonaco()
      .then((monaco) => {
        if (disposed) return;
        const current = latest.current;
        const document = monaco.editor.createModel(current.value, languageId(current.language));
        const view = monaco.editor.create(host.current!, {
          model: document,
          automaticLayout: true,
          readOnly: current.disabled,
          domReadOnly: current.disabled,
          minimap: { enabled: false },
          contextmenu: true,
          quickSuggestions: { other: true, comments: false, strings: false },
          suggestOnTriggerCharacters: true,
          wordBasedSuggestions: "currentDocument",
          tabCompletion: "on",
          suggest: { showWords: true, showSnippets: true },
          fontSize: 14,
          fontFamily: OJ_MONO_FONT_FAMILY,
          wordWrap: "on",
          scrollBeyondLastLine: false,
          ariaLabel: current.ariaLabel ?? "代码编辑器",
          fixedOverflowWidgets: true,
        });
        editor.current = view;
        model.current = document;
        const listener = document.onDidChangeContent(() => {
          if (!external.current && !latest.current.disabled)
            latest.current.onChange(document.getValue());
        });
        const root = window.document.documentElement;
        const theme = () => {
          const isDark =
            root.classList.contains("dark") ||
            root.dataset.theme === "dark" ||
            root.getAttribute("data-theme-mode") === "dark";
          monaco.editor.setTheme(isDark ? "vitesse-dark" : "vitesse-light");
        };
        theme();
        const observer = new MutationObserver(theme);
        observer.observe(root, {
          attributes: true,
          attributeFilter: ["class", "data-theme", "data-theme-mode"],
        });
        release = () => {
          observer.disconnect();
          listener.dispose();
          view.dispose();
          document.dispose();
          editor.current = undefined;
          model.current = undefined;
        };
        setReady(true);
      })
      .catch((e) => {
        if (!disposed) setError(`代码编辑器加载失败：${String(e)}`);
      });
    return () => {
      disposed = true;
      release?.();
    };
    // Modern Monaco derives the immutable model URI extension from its language.
    // Recreate it on language changes so JS/TS workers never receive a .cpp model.
  }, [attempt, language]);
  useEffect(() => {
    if (model.current && model.current.getValue() !== value) {
      external.current = true;
      model.current.setValue(value);
      external.current = false;
    }
  }, [value]);
  useEffect(() => {
    editor.current?.updateOptions({ readOnly: disabled, domReadOnly: disabled, ariaLabel });
  }, [disabled, ariaLabel]);
  return (
    <div>
      {error && (
        <Banner status="danger">
          {error}
          <Button variant="tonal" onClick={() => setAttempt((v) => v + 1)}>
            重试加载编辑器
          </Button>
        </Banner>
      )}
      {!ready && !error && <LoadingIndicator size={24} aria-label="加载代码编辑器" />}
      <div ref={host} className="oj-monaco-editor" data-editor="modern-monaco" />
    </div>
  );
}
