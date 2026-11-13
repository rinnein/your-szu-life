/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";
import { setStoredHtmlTagVisibility } from "./DimmedHtmlText";
import { RichText, countRichImages, plainTextFromRich } from "./RichText";

describe("RichText", () => {
  beforeEach(() => {
    cleanup();
    localStorage.clear();
    setStoredHtmlTagVisibility(true);
  });

  afterEach(() => {
    cleanup();
  });

  it("renders markdown images through the bank image map", () => {
    const { container } = render(
      <RichText
        text="如图 ![曲线图](#<pic_1>) 求面积"
        images={[{ pic_id: "pic_1", url: "https://a.com/1.png" }]}
      />,
    );
    const image = container.querySelector("img");
    expect(image).not.toBeNull();
    expect(image).toHaveAttribute("src", "https://a.com/1.png");
    expect(image).toHaveAttribute("alt", "曲线图");
    expect(screen.getByText(/如图/)).toBeInTheDocument();
  });

  it("accepts camelCase picId and unuploaded numeric temp ids", () => {
    const { container } = render(
      <RichText
        text="![甲](#<1>) 与 ![乙](#<2>)"
        images={[
          { picId: "1", url: "https://a.com/1.png" },
          { pic_id: "2", url: "data:image/png;base64,AAAA" },
        ]}
      />,
    );
    const images = container.querySelectorAll("img");
    expect(images).toHaveLength(2);
    expect(images[0]).toHaveAttribute("src", "https://a.com/1.png");
    expect(images[1]).toHaveAttribute("src", "data:image/png;base64,AAAA");
  });

  it("prefers runtime data images and supports an offset", () => {
    const { container } = render(
      <RichText
        text="![题干图](https://a.com/1.png) 与 ![题干图2](https://a.com/2.png)"
        dataImages={["data:image/png;base64,AAAA", "data:image/png;base64,BBBB"]}
        imageOffset={0}
      />,
    );
    const images = container.querySelectorAll("img");
    expect(images[0]).toHaveAttribute("src", "data:image/png;base64,AAAA");
    expect(images[1]).toHaveAttribute("src", "data:image/png;base64,BBBB");
  });

  it("falls back to the markdown url when the runtime slot is an empty placeholder", () => {
    const { container } = render(
      <RichText text="![图](https://a.com/1.png)" dataImages={[""]} imageOffset={0} />,
    );
    expect(container.querySelector("img")).toHaveAttribute("src", "https://a.com/1.png");
  });

  it("falls back to alt text when the reference cannot be resolved", () => {
    const { container } = render(<RichText text="如图 ![缺失图片](#<missing>) 所示" />);
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText(/缺失图片/)).toBeInTheDocument();
  });

  it("keeps html tags dimmed for legacy question text", () => {
    const { container } = render(<RichText text="<p>旧数据题干</p>" />);
    expect(screen.getByText("旧数据题干")).toBeInTheDocument();
    expect(container.querySelectorAll(".font-mono").length).toBeGreaterThan(0);
  });

  it("counts rich images and produces plain text labels", () => {
    const text = "![甲](https://a.com/1.png) 题干 ![乙](#<2>)";
    expect(countRichImages(text)).toBe(2);
    expect(plainTextFromRich(text)).toBe("甲 题干 乙");
    expect(plainTextFromRich("<p>题干&nbsp;内容</p>")).toBe("题干 内容");
  });
});
