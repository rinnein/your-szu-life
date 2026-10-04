import { useEffect, useRef } from "react";
import * as monaco from "monaco-editor/editor/editor.api.js";
import "monaco-editor/languages/definitions/cpp/register.js";
import "monaco-editor/languages/definitions/java/register.js";
import "monaco-editor/languages/definitions/python/register.js";
import "monaco-editor/languages/definitions/go/register.js";
import "monaco-editor/languages/definitions/javascript/register.js";
import "monaco-editor/languages/features/typescript/register.js";
import "monaco-editor/editor/browser/coreCommands.js";
import "monaco-editor/editor/contrib/find/browser/findController.js";
import "monaco-editor/editor/contrib/suggest/browser/suggestController.js";
import "monaco-editor/editor/contrib/wordOperations/browser/wordOperations.js";
import "monaco-editor/editor/contrib/bracketMatching/browser/bracketMatching.js";
import "monaco-editor/editor/contrib/contextmenu/browser/contextmenu.js";
import "monaco-editor/editor/standalone/browser/quickAccess/standaloneCommandsQuickAccess.js";
import "monaco-editor/editor/contrib/multicursor/browser/multicursor.js";
import "../../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.css";
import "../../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon-modifiers.css";
import "monaco-editor/editor/contrib/hover/browser/hoverContribution.js";
import "monaco-editor/editor/contrib/parameterHints/browser/parameterHints.js";
import "monaco-editor/editor/contrib/snippet/browser/snippetController2.js";
import "monaco-editor/editor/contrib/clipboard/browser/clipboard.js";
import "monaco-editor/editor/contrib/comment/browser/comment.js";
import "monaco-editor/editor/contrib/format/browser/formatActions.js";
import { registerOjCompletions } from "./monaco-completions";
import EditorWorker from "monaco-editor/editor/editor.worker.js?worker";
import TypeScriptWorker from "monaco-editor/language/typescript/ts.worker.js?worker";
import type { CodeEditorProps } from "./CodeEditor";
(self as typeof self & { MonacoEnvironment: monaco.Environment }).MonacoEnvironment = {
  getWorker: (_id, label) =>
    label === "typescript" || label === "javascript" ? new TypeScriptWorker() : new EditorWorker(),
};
registerOjCompletions(monaco);
const languageId = (language: string) =>
  ({ c: "c", cpp: "cpp", java: "java", py: "python", go: "go", js: "javascript" })[language] ??
  "plaintext";
export default function MonacoEditor({ value, language, disabled, onChange }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<monaco.editor.IStandaloneCodeEditor>(undefined);
  const model = useRef<monaco.editor.ITextModel>(undefined);
  const changed = useRef(onChange);
  changed.current = onChange;
  const external = useRef(false);
  useEffect(() => {
    const document = monaco.editor.createModel(value, languageId(language));
    model.current = document;
    const view = monaco.editor.create(host.current!, {
      model: document,
      automaticLayout: true,
      readOnly: disabled,
      minimap: { enabled: false },
      contextmenu: true,
      quickSuggestions: { other: true, comments: false, strings: false },
      suggestOnTriggerCharacters: true,
      wordBasedSuggestions: "matchingDocuments",
      tabCompletion: "on",
      suggest: { showWords: true, showSnippets: true },
      fontSize: 14,
      wordWrap: "on",
      scrollBeyondLastLine: false,
      ariaLabel: "代码编辑器",
      fixedOverflowWidgets: true,
    });
    editor.current = view;
    const listener = document.onDidChangeContent(() => {
      if (!external.current) changed.current(document.getValue());
    });
    const root = window.document.documentElement;
    const theme = () =>
      monaco.editor.setTheme(
        root.classList.contains("dark") || root.dataset.theme === "dark" ? "vs-dark" : "vs",
      );
    theme();
    const observer = new MutationObserver(theme);
    observer.observe(root, { attributes: true, attributeFilter: ["class", "data-theme"] });
    return () => {
      observer.disconnect();
      listener.dispose();
      view.dispose();
      document.dispose();
      editor.current = undefined;
      model.current = undefined;
    };
  }, []);
  useEffect(() => {
    if (model.current && model.current.getValue() !== value) {
      external.current = true;
      model.current.setValue(value);
      external.current = false;
    }
  }, [value]);
  useEffect(() => {
    if (model.current) monaco.editor.setModelLanguage(model.current, languageId(language));
  }, [language]);
  useEffect(() => {
    editor.current?.updateOptions({ readOnly: disabled });
  }, [disabled]);
  return <div ref={host} className="oj-monaco-editor" data-editor="monaco" />;
}
