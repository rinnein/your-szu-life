/** @vitest-environment jsdom */
import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vite-plus/test";
import { CompileOptions } from "./CompileOptions";
import { compileOptions } from "./compile-options";
afterEach(cleanup);
it("defaults to C17/C++20 and hides options for other languages", () => {
  expect(compileOptions("c")?.version).toBe("c17");
  expect(compileOptions("cpp")?.version).toBe("c++20");
  expect(compileOptions("py")).toBeNull();
  render(<CompileOptions language="py" value={null} disabled={false} onChange={() => {}} />);
  expect(screen.queryByRole("group")).toBeNull();
});
it("requires ASan for LSan and clears LSan when ASan is disabled", () => {
  function Form() {
    const [value, setValue] = useState(compileOptions("cpp"));
    return <CompileOptions language="cpp" value={value} onChange={setValue} disabled={false} />;
  }
  render(<Form />);
  const asan = screen.getByRole("switch", { name: /ASan ·/ });
  const lsan = screen.getByRole("switch", { name: /LSan ·/ }) as HTMLInputElement;
  expect(lsan.disabled).toBe(true);
  fireEvent.click(asan);
  expect(lsan.disabled).toBe(false);
  fireEvent.click(lsan);
  expect(lsan.checked).toBe(true);
  fireEvent.click(asan);
  expect(lsan.disabled).toBe(true);
  expect(lsan.checked).toBe(false);
});
it("preserves saved language-specific standards", () => {
  expect(compileOptions("cpp", { version: "c++14", enableAsan: true, enableLsan: true })).toEqual({
    version: "c++14",
    enableAsan: true,
    enableLsan: true,
  });
  expect(compileOptions("c", { version: "c++20", enableAsan: false, enableLsan: true })).toEqual({
    version: "c17",
    enableAsan: false,
    enableLsan: false,
  });
});
