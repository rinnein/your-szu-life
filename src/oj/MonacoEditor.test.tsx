/** @vitest-environment jsdom */
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vite-plus/test";
const mock = vi.hoisted(() => ({ create: vi.fn(), createModel: vi.fn(), setTheme: vi.fn() }));
vi.mock("modern-monaco", () => ({ init: vi.fn().mockResolvedValue({ editor: mock }) }));
vi.mock("./monaco-completions", () => ({ registerOjCompletions: vi.fn() }));
import MonacoEditor from "./MonacoEditor";
afterEach(cleanup);
it("recreates and disposes models when switching languages, preserving the supplied draft and read-only state", async () => {
  const models: { dispose: ReturnType<typeof vi.fn> }[] = [];
  mock.createModel.mockImplementation((value) => {
    const model = {
      dispose: vi.fn(),
      getValue: () => value,
      setValue: vi.fn(),
      onDidChangeContent: () => ({ dispose: vi.fn() }),
    };
    models.push(model);
    return model;
  });
  const destroy = vi.fn();
  mock.create.mockReturnValue({ dispose: destroy, updateOptions: vi.fn() });
  const view = render(
    <MonacoEditor value="int main(){}" language="cpp" disabled={false} onChange={() => {}} />,
  );
  await waitFor(() => expect(mock.createModel).toHaveBeenCalledWith("int main(){}", "cpp"));
  view.rerender(
    <MonacoEditor value="const value = 1" language="js" disabled onChange={() => {}} />,
  );
  await waitFor(() =>
    expect(mock.createModel).toHaveBeenCalledWith("const value = 1", "javascript"),
  );
  expect(models[0].dispose).toHaveBeenCalledOnce();
  expect(mock.create).toHaveBeenLastCalledWith(
    expect.anything(),
    expect.objectContaining({ readOnly: true, domReadOnly: true }),
  );
  view.unmount();
  expect(models[1].dispose).toHaveBeenCalledOnce();
  expect(destroy).toHaveBeenCalledTimes(2);
});
