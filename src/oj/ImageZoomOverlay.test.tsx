/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { installAndroidBack, type AndroidBackBridge } from "../android-back";
import { Html } from "./OjPaper";
import { ImageZoomHost, shouldCloseOnScrimClick } from "./ImageZoomOverlay";
import {
  closeImageZoom,
  getImageZoomSnapshot,
  openImageZoom,
  subscribeImageZoom,
} from "./image-zoom";

afterEach(() => {
  cleanup();
  closeImageZoom();
});

function makeImageLoaded(img: HTMLImageElement, naturalWidth = 100) {
  Object.defineProperty(img, "complete", { value: true });
  Object.defineProperty(img, "naturalWidth", { value: naturalWidth });
}

/** 打开请求已经发出后的浮层断言；Host 必须由调用方先 render。 */
async function openOverlay(src = "data:image/png;base64,Zm9v", alt = "题目示意图") {
  act(() => openImageZoom({ src, alt }));
  return screen.findByRole("dialog", { name: "查看题目图片" });
}

describe("image zoom bridge", () => {
  it("publishes requests to subscribers and clears on close", () => {
    const updates: (string | undefined)[] = [];
    const unsubscribe = subscribeImageZoom(() => updates.push(getImageZoomSnapshot()?.src));
    openImageZoom({ src: "a.png" });
    expect(getImageZoomSnapshot()?.src).toBe("a.png");
    closeImageZoom();
    expect(getImageZoomSnapshot()).toBeUndefined();
    unsubscribe();
    openImageZoom({ src: "b.png" });
    expect(updates).toEqual(["a.png", undefined]);
    closeImageZoom();
  });

  it("keeps the snapshot reference stable between requests", () => {
    openImageZoom({ src: "a.png" });
    const first = getImageZoomSnapshot();
    expect(getImageZoomSnapshot()).toBe(first);
    closeImageZoom();
  });
});

describe("image zoom overlay shell", () => {
  it("renders the requested image inside the portal", async () => {
    render(<ImageZoomHost />);
    await openOverlay();
    const image = document.querySelector<HTMLImageElement>(".oj-image-zoom-content img");
    expect(image).toHaveAttribute("src", "data:image/png;base64,Zm9v");
    expect(image).toHaveAttribute("alt", "题目示意图");
    // android-back 的 role="dialog" 兜底依赖这个标记。
    expect(screen.getByRole("dialog")).toBe(document.querySelector(".oj-image-zoom"));
  });

  it("locks body scroll while open and restores it after closing", async () => {
    render(<ImageZoomHost />);
    document.body.style.overflow = "";
    await openOverlay();
    expect(document.body).toHaveStyle({ overflow: "hidden" });
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(document.body.style.overflow).toBe("");
  });

  it("closes on the close button and on Escape", async () => {
    render(<ImageZoomHost />);
    await openOverlay();
    fireEvent.click(screen.getByRole("button", { name: "关闭图片查看" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await openOverlay();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("closes on empty-area clicks at fit scale but not inside the image box", async () => {
    render(<ImageZoomHost />);
    await openOverlay();
    fireEvent.click(document.querySelector(".oj-image-zoom-content")!);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("dialog"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("never closes when zoomed past fit scale", () => {
    const content = document.createElement("div");
    content.className = "oj-image-zoom-content";
    const button = document.createElement("button");
    expect(shouldCloseOnScrimClick(1, null)).toBe(true);
    expect(shouldCloseOnScrimClick(1.2, null)).toBe(false);
    expect(shouldCloseOnScrimClick(2.5, content)).toBe(false);
    expect(shouldCloseOnScrimClick(1, button)).toBe(false);
  });

  it("closes through the Android system back via the role=dialog fallback", async () => {
    const bridge: AndroidBackBridge = { postMessage: vi.fn(), onmessage: null };
    window.SzuApplicationBack = bridge;
    const dispose = installAndroidBack({
      canBack: () => false,
      goBack: vi.fn(async () => {}),
      subscribe: () => () => {},
    });
    try {
      render(<ImageZoomHost />);
      await openOverlay();
      await act(async () => {
        bridge.onmessage?.({ data: "dismiss-overlay" });
      });
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    } finally {
      dispose();
      delete window.SzuApplicationBack;
    }
  });
});

describe("problem statement image delegation", () => {
  it("opens the overlay for a loaded image", async () => {
    render(
      <>
        <Html
          html='<p><img src="https://oj.example/img/a.png" alt="题图"></p>'
          base="https://oj.example/contest/1"
        />
        <ImageZoomHost />
      </>,
    );
    const img = document.querySelector(".oj-statement img")!;
    makeImageLoaded(img as HTMLImageElement);
    fireEvent.click(img);
    await waitFor(() =>
      expect(document.querySelector(".oj-image-zoom-content img")).toHaveAttribute(
        "src",
        "https://oj.example/img/a.png",
      ),
    );
  });

  it("ignores images wrapped in a link", () => {
    render(
      <>
        <Html
          html='<p><a href="https://oj.example/img/a.png"><img src="https://oj.example/img/a.png" alt="题图"></a></p>'
          base="https://oj.example/contest/1"
        />
        <ImageZoomHost />
      </>,
    );
    const img = document.querySelector(".oj-statement img")!;
    makeImageLoaded(img as HTMLImageElement);
    fireEvent.click(img);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("ignores images that have not finished loading", () => {
    render(
      <>
        <Html
          html='<p><img src="https://oj.example/img/a.png" alt="题图"></p>'
          base="https://oj.example/contest/1"
        />
        <ImageZoomHost />
      </>,
    );
    const img = document.querySelector(".oj-statement img") as HTMLImageElement;
    makeImageLoaded(img, 0);
    fireEvent.click(img);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
