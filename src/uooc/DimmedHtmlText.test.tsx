/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vite-plus/test";
import { DimmedHtmlText } from "./DimmedHtmlText";

describe("DimmedHtmlText", () => {
  it("renders text without tags normally", () => {
    const { container } = render(<DimmedHtmlText text="纯文本题干内容" />);
    expect(screen.getByText("纯文本题干内容")).toBeInTheDocument();
    expect(container.querySelectorAll(".font-mono")).toHaveLength(0);
  });

  it("renders HTML tags in dimmed styling while keeping inner text normal", () => {
    const { container } = render(
      <DimmedHtmlText text="<p>关于微积分基本定理，<span class='highlight'>下列说法正确的是？</span></p>" />,
    );

    expect(screen.getByText("关于微积分基本定理，")).toBeInTheDocument();
    expect(screen.getByText("下列说法正确的是？")).toBeInTheDocument();

    const dimmedTags = container.querySelectorAll(".font-mono");
    expect(dimmedTags.length).toBeGreaterThan(0);
    expect(screen.getByText("<p>")).toHaveClass("text-muted/40", "font-mono");
    expect(screen.getByText("</p>")).toHaveClass("text-muted/40", "font-mono");
    expect(screen.getByText("<span class='highlight'>")).toHaveClass("text-muted/40", "font-mono");
    expect(screen.getByText("</span>")).toHaveClass("text-muted/40", "font-mono");
  });

  it("handles null or empty string gracefully", () => {
    const { container } = render(<DimmedHtmlText text="" />);
    expect(container.firstChild).toBeNull();
  });
});
