/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";
import {
  DimmedHtmlText,
  setStoredHtmlTagVisibility,
} from "./DimmedHtmlText";

describe("DimmedHtmlText", () => {
  beforeEach(() => {
    cleanup();
    localStorage.clear();
    setStoredHtmlTagVisibility(true);
  });

  afterEach(() => {
    cleanup();
  });

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

  it("handles nested tags, inline style attributes and self-closing tags", () => {
    const { container } = render(
      <DimmedHtmlText text="<p style='margin:0'><span style='color:red;'><strong>嵌套加粗</strong></span><br/></p>" />,
    );

    expect(screen.getByText("嵌套加粗")).toBeInTheDocument();
    expect(screen.getByText("<p style='margin:0'>")).toBeInTheDocument();
    expect(screen.getByText("<span style='color:red;'>")).toBeInTheDocument();
    expect(screen.getByText("<strong>")).toBeInTheDocument();
    expect(screen.getByText("</strong>")).toBeInTheDocument();
    expect(screen.getByText("</span>")).toBeInTheDocument();
    expect(screen.getByText("<br/>")).toBeInTheDocument();
    expect(screen.getByText("</p>")).toBeInTheDocument();
    expect(container.querySelectorAll(".font-mono")).toHaveLength(7);
  });

  it("does not mistake mathematical comparison operators as HTML tags", () => {
    const { container } = render(
      <DimmedHtmlText text="若 x < y 且 z > 0，则下列结论正确的是？" />,
    );

    expect(screen.getByText("若 x < y 且 z > 0，则下列结论正确的是？")).toBeInTheDocument();
    expect(container.querySelectorAll(".font-mono")).toHaveLength(0);
  });

  it("hides dimmed HTML tags completely when showTags prop is false", () => {
    const { container } = render(
      <DimmedHtmlText
        text="<p>关于微积分基本定理，<span style='color:red'>下列说法正确的是？</span></p>"
        showTags={false}
      />,
    );

    expect(screen.getByText("关于微积分基本定理，")).toBeInTheDocument();
    expect(screen.getByText("下列说法正确的是？")).toBeInTheDocument();
    // All dimmed tag elements should be hidden
    expect(container.querySelectorAll(".font-mono")).toHaveLength(0);
    expect(screen.queryByText("<p>")).toBeNull();
    expect(screen.queryByText("</p>")).toBeNull();
  });

  it("toggles HTML tag visibility dynamically via setStoredHtmlTagVisibility", () => {
    const { container } = render(
      <DimmedHtmlText text="<p>题干内容<br/></p>" />,
    );

    expect(container.querySelectorAll(".font-mono")).toHaveLength(3);

    act(() => {
      setStoredHtmlTagVisibility(false);
    });

    expect(container.querySelectorAll(".font-mono")).toHaveLength(0);
    expect(screen.getByText("题干内容")).toBeInTheDocument();

    act(() => {
      setStoredHtmlTagVisibility(true);
    });

    expect(container.querySelectorAll(".font-mono")).toHaveLength(3);
  });

  it("handles null or empty string gracefully", () => {
    const { container } = render(<DimmedHtmlText text="" />);
    expect(container.firstChild).toBeNull();
  });
});

