import type * as Monaco from "monaco-editor/editor/editor.api.js";

// Monaco only ships a language service for JS/TS. These offline keyword/snippet
// suggestions supplement its document-word completion for the other OJ languages.
const keywords: Record<string, string[]> = {
  c: "auto break case char const continue default do double else enum extern float for goto if int long register return short signed sizeof static struct switch typedef union unsigned void volatile while".split(
    " ",
  ),
  cpp: "alignas auto bool break case catch char class const constexpr continue decltype default delete do double else enum explicit false float for friend if inline int long namespace new noexcept nullptr operator private protected public return short signed sizeof static static_assert struct switch template this throw true try typedef typename union unsigned using virtual void volatile while".split(
    " ",
  ),
  java: "abstract boolean break byte case catch char class continue default do double else enum extends final finally float for if implements import int interface long new package private protected public return short static super switch synchronized this throw throws try void while".split(
    " ",
  ),
  python:
    "and as assert async await break class continue def del elif else except False finally for from global if import in is lambda None nonlocal not or pass raise return True try while with yield".split(
      " ",
    ),
  go: "break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var".split(
    " ",
  ),
};
export interface OjSnippet {
  label: string;
  body: string;
  description: string;
}
export const snippets: Record<string, OjSnippet[]> = {
  c: [
    { label: "main", body: "int main(void) {\n\t$0\n\treturn 0;\n}", description: "C main 函数" },
    {
      label: "fori",
      body: "for (int ${1:i} = 0; ${1:i} < ${2:n}; ++${1:i}) {\n\t$0\n}",
      description: "计数循环",
    },
    { label: "include", body: "#include <${1:stdio.h}>", description: "引入头文件" },
  ],
  cpp: [
    {
      label: "main",
      body: "int main() {\n\tstd::ios::sync_with_stdio(false);\n\tstd::cin.tie(nullptr);\n\t$0\n\treturn 0;\n}",
      description: "C++ main 函数与快速 I/O",
    },
    {
      label: "fori",
      body: "for (int ${1:i} = 0; ${1:i} < ${2:n}; ++${1:i}) {\n\t$0\n}",
      description: "计数循环",
    },
    {
      label: "vector",
      body: "std::vector<${1:int}> ${2:values};",
      description: "std::vector 容器",
    },
    { label: "include", body: "#include <${1:iostream}>", description: "引入头文件" },
  ],
  java: [
    {
      label: "main",
      body: "public static void main(String[] args) {\n\t$0\n}",
      description: "Java main 方法",
    },
    {
      label: "fori",
      body: "for (int ${1:i} = 0; ${1:i} < ${2:n}; ++${1:i}) {\n\t$0\n}",
      description: "计数循环",
    },
  ],
  python: [
    { label: "main", body: 'if __name__ == "__main__":\n\t$0', description: "Python 主入口" },
    { label: "fori", body: "for ${1:i} in range(${2:n}):\n\t$0", description: "range 循环" },
    { label: "def", body: "def ${1:solve}(${2:}):\n\t$0", description: "函数定义" },
  ],
  go: [
    { label: "main", body: "func main() {\n\t$0\n}", description: "Go main 函数" },
    {
      label: "fori",
      body: "for ${1:i} := 0; ${1:i} < ${2:n}; ${1:i}++ {\n\t$0\n}",
      description: "计数循环",
    },
  ],
};
const registered = new WeakSet<object>();
export function registerOjCompletions(monaco: typeof Monaco) {
  if (registered.has(monaco.languages)) return;
  registered.add(monaco.languages);
  monaco.languages.registerCompletionItemProvider(Object.keys(keywords), {
    triggerCharacters: [":", "#"],
    provideCompletionItems(model, position) {
      const language = model.getLanguageId();
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      const prefix = model.getLineContent(position.lineNumber).slice(0, word.startColumn - 1);
      return {
        suggestions: [
          ...(keywords[language] ?? []).map((keyword) => ({
            label: keyword,
            kind: monaco.languages.CompletionItemKind.Keyword,
            insertText: keyword,
            range,
          })),
          ...(snippets[language] ?? []).map((snippet) => ({
            label: snippet.label,
            detail: snippet.description,
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            insertText: prefix.endsWith("std::")
              ? snippet.body.replace(/^std::/, "")
              : prefix.endsWith("#")
                ? snippet.body.replace(/^#/, "")
                : snippet.body,
            range,
          })),
        ],
      };
    },
  });
}
