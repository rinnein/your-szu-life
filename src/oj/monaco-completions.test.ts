import { expect, it, vi } from "vite-plus/test";
import type * as Monaco from "modern-monaco/editor-core";
import { registerOjCompletions } from "./monaco-completions";
it("offers usable C++ snippets without duplicating a typed std prefix", () => {
  const register = vi.fn();
  const monaco = {
    languages: {
      registerCompletionItemProvider: register,
      CompletionItemKind: { Keyword: 1, Snippet: 2 },
      CompletionItemInsertTextRule: { InsertAsSnippet: 4 },
    },
  } as unknown as typeof Monaco;
  registerOjCompletions(monaco);
  registerOjCompletions(monaco);
  expect(register).toHaveBeenCalledTimes(1);
  const provider = register.mock.calls[0][1];
  const result = provider.provideCompletionItems(
    {
      getLanguageId: () => "cpp",
      getWordUntilPosition: () => ({ startColumn: 6, endColumn: 9 }),
      getLineContent: () => "std::vec",
    },
    { lineNumber: 1, column: 9 },
  );
  const vector = result.suggestions.find((v: { label: string }) => v.label === "vector");
  expect(vector.insertText).toBe("vector<${1:int}> ${2:values};");
  expect(vector.range).toEqual({
    startLineNumber: 1,
    endLineNumber: 1,
    startColumn: 6,
    endColumn: 9,
  });
  expect(result.suggestions.some((v: { label: string }) => v.label === "constexpr")).toBe(true);
});
it("does not duplicate the include marker and provides placeholders", () => {
  const register = vi.fn();
  const monaco = {
    languages: {
      registerCompletionItemProvider: register,
      CompletionItemKind: { Keyword: 1, Snippet: 2 },
      CompletionItemInsertTextRule: { InsertAsSnippet: 4 },
    },
  } as unknown as typeof Monaco;
  registerOjCompletions(monaco);
  const result = register.mock.calls[0][1].provideCompletionItems(
    {
      getLanguageId: () => "c",
      getWordUntilPosition: () => ({ startColumn: 2, endColumn: 5 }),
      getLineContent: () => "#inc",
    },
    { lineNumber: 1, column: 5 },
  );
  expect(result.suggestions.find((v: { label: string }) => v.label === "include").insertText).toBe(
    "include <${1:stdio.h}>",
  );
});
