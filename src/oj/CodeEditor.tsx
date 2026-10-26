import { useEffect, useRef } from "react";
import { EditorView, basicSetup } from "codemirror";
import { EditorState, Compartment } from "@codemirror/state";
import { cpp } from "@codemirror/lang-cpp";
import { java } from "@codemirror/lang-java";
import { python } from "@codemirror/lang-python";
import { javascript } from "@codemirror/lang-javascript";
import { go } from "@codemirror/lang-go";
const syntax = (language: string) =>
  language === "java"
    ? java()
    : language === "py"
      ? python()
      : language === "go"
        ? go()
        : language === "js"
          ? javascript()
          : cpp();

const darkTheme = EditorView.theme(
  {
    "&": {
      color: "var(--mtrl-sys-color-on-surface)",
      backgroundColor: "var(--mtrl-sys-color-surface-container-low)",
    },
    ".cm-content": {
      caretColor: "var(--mtrl-sys-color-primary)",
    },
    "&.cm-focused .cm-cursor": {
      borderLeftColor: "var(--mtrl-sys-color-primary)",
    },
    "&.cm-focused .cm-selectionBackground, ::selection": {
      backgroundColor: "color-mix(in srgb, var(--mtrl-sys-color-primary) 30%, transparent)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--mtrl-sys-color-surface-container-lowest)",
      color: "var(--mtrl-sys-color-on-surface-variant)",
      borderRight: "1px solid var(--mtrl-sys-color-outline-variant)",
    },
  },
  { dark: true },
);

export interface CodeEditorProps {
  value: string;
  language: string;
  disabled: boolean;
  onChange: (text: string) => void;
  ariaLabel?: string;
}
export function CodeEditor({
  value,
  language,
  disabled,
  onChange,
  ariaLabel = "代码编辑器",
}: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<EditorView>(undefined);
  const changed = useRef(onChange);
  changed.current = onChange;
  const config = useRef(new Compartment());
  const themeCompartment = useRef(new Compartment());
  const external = useRef(false);
  useEffect(() => {
    const isDark =
      typeof document !== "undefined" &&
      (document.documentElement.getAttribute("data-theme-mode") === "dark" ||
        document.documentElement.classList.contains("dark"));
    const view = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          config.current.of([
            syntax(language),
            EditorState.readOnly.of(disabled),
            EditorView.editable.of(!disabled),
          ]),
          themeCompartment.current.of(isDark ? darkTheme : []),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({
            "aria-label": ariaLabel,
            "aria-readonly": String(disabled),
          }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged && !external.current)
              changed.current(update.state.doc.toString());
          }),
        ],
      }),
    });
    editor.current = view;
    return () => {
      view.destroy();
      editor.current = undefined;
    };
  }, []);
  useEffect(() => {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    const checkTheme = () => {
      const isDark =
        root.getAttribute("data-theme-mode") === "dark" || root.classList.contains("dark");
      editor.current?.dispatch({
        effects: themeCompartment.current.reconfigure(isDark ? darkTheme : []),
      });
    };
    const observer = new MutationObserver(checkTheme);
    observer.observe(root, { attributes: true, attributeFilter: ["data-theme-mode", "class"] });
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const view = editor.current;
    if (view && view.state.doc.toString() !== value) {
      external.current = true;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
      external.current = false;
    }
  }, [value]);
  useEffect(() => {
    editor.current?.dispatch({
      effects: config.current.reconfigure([
        syntax(language),
        EditorState.readOnly.of(disabled),
        EditorView.editable.of(!disabled),
      ]),
    });
  }, [language, disabled]);
  return <div ref={host} className="oj-code-editor" />;
}
