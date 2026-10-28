/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vite-plus/test";
import { ThemeProvider, useTheme } from "./ThemeProvider";
import { DEFAULT_APPEARANCE } from "./defaults";
import { THEME_STYLE_ID } from "./tokens";

afterEach(cleanup);

function ModeProbe() {
  const { resolvedMode } = useTheme();
  return <span data-testid="mode">{resolvedMode}</span>;
}

it("applies the theme css and the resolved mode to the document", () => {
  render(
    <ThemeProvider initial={{ ...DEFAULT_APPEARANCE, mode: "dark" }}>
      <ModeProbe />
    </ThemeProvider>,
  );

  expect(screen.getByTestId("mode")).toHaveTextContent("dark");
  expect(document.documentElement.dataset.themeMode).toBe("dark");
  expect(document.documentElement.dataset.theme).toBe("szu");
  // 系统栏取色由 src/shell/native-chrome 直接读控件颜色，主题这里只负责写 CSS。
  expect(document.getElementById(THEME_STYLE_ID)?.textContent).toContain(
    '[data-theme-mode="dark"]{',
  );
});
