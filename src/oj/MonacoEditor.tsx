import { useEffect, useRef, useState } from "react";
import { Alert, Button, Spinner } from "@heroui/react";
import { init } from "modern-monaco";
import type * as Monaco from "modern-monaco/editor-core";
import { registerOjCompletions } from "./monaco-completions";
import type { CodeEditorProps } from "./CodeEditor";
let loading: Promise<typeof Monaco> | undefined;
function loadMonaco() {
  return (loading ??= init({
    cdn: window.location.origin,
    defaultTheme: "vitesse-light",
    themes: ["vitesse-dark"],
    langs: ["c", "cpp", "java", "python", "go"],
  })
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
        const theme = () =>
          monaco.editor.setTheme(
            root.classList.contains("dark") || root.dataset.theme === "dark"
              ? "vitesse-dark"
              : "vitesse-light",
          );
        theme();
        const observer = new MutationObserver(theme);
        observer.observe(root, { attributes: true, attributeFilter: ["class", "data-theme"] });
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
        <Alert status="danger">
          <Alert.Content>
            {error}
            <Button variant="secondary" onPress={() => setAttempt((v) => v + 1)}>
              重试加载编辑器
            </Button>
          </Alert.Content>
        </Alert>
      )}
      {!ready && !error && <Spinner aria-label="加载代码编辑器" />}
      <div ref={host} className="oj-monaco-editor" data-editor="modern-monaco" />
    </div>
  );
}
