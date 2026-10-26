/** @vitest-environment jsdom */
import "../test-setup-element-internals";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import {
  Avatar,
  Banner,
  Disclosure,
  Kbd,
  NumberField,
  Pagination,
  resetToasts,
  Table,
  toast,
} from "./index";

afterEach(() => cleanup());

describe("Banner", () => {
  it("uses alert role for danger", () => {
    render(
      <Banner status="danger">
        <Banner.Title>出错</Banner.Title>
        <Banner.Description>详情</Banner.Description>
      </Banner>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("出错");
  });

  it("uses status role otherwise", () => {
    render(
      <Banner status="success">
        <Banner.Title>成功</Banner.Title>
      </Banner>,
    );
    expect(screen.getByRole("status")).toHaveTextContent("成功");
  });

  it("applies the status palette class", () => {
    const { container } = render(
      <>
        <Banner status="warning">W</Banner>
        <Banner status="accent">A</Banner>
        <Banner>D</Banner>
      </>,
    );
    const banners = Array.from(container.querySelectorAll(".banner"));
    expect(banners[0]).toHaveClass("banner-warning");
    expect(banners[1]).toHaveClass("banner-accent");
    expect(banners[2]).toHaveClass("banner-default");
  });
});

describe("Table", () => {
  it("renders a real table with header cells", () => {
    render(
      <Table.ScrollContainer>
        <Table.Content aria-label="成绩">
          <Table.Header>
            <Table.Column>课程</Table.Column>
            <Table.Column>分数</Table.Column>
          </Table.Header>
          <Table.Body>
            <Table.Row>
              <Table.Cell>高数</Table.Cell>
              <Table.Cell>95</Table.Cell>
            </Table.Row>
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>,
    );
    const table = screen.getByRole("grid", { name: "成绩" });
    expect(table).toBeInTheDocument();
    expect(screen.getAllByRole("columnheader")).toHaveLength(2);
    expect(screen.getAllByRole("cell")).toHaveLength(2);
    expect(screen.getByText("高数")).toBeInTheDocument();
  });

  it("makes clickable rows keyboard accessible", () => {
    const onClick = vi.fn();
    render(
      <Table.Content aria-label="列表">
        <Table.Body>
          <Table.Row onClick={onClick}>
            <Table.Cell>可点击</Table.Cell>
          </Table.Row>
        </Table.Body>
      </Table.Content>,
    );
    const row = screen.getByRole("row");
    expect(row).toHaveAttribute("tabindex", "0");
  });
});

describe("Disclosure", () => {
  it("hides the panel until expanded", () => {
    render(<Disclosure title="更多">隐藏内容</Disclosure>);
    // 收起时不渲染面板内容：面板里常放表格等
    // 重内容，留在无障碍树里会让读屏读到不可见内容。
    expect(screen.queryByText("隐藏内容")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /更多/ })).toHaveAttribute("aria-expanded", "false");
  });

  it("shows content when defaultOpen", () => {
    render(
      <Disclosure title="更多" defaultOpen>
        隐藏内容
      </Disclosure>,
    );
    expect(screen.getByText("隐藏内容")).toBeInTheDocument();
  });
});

describe("Avatar", () => {
  it("renders a fallback with an accessible name", () => {
    render(<Avatar aria-label="张三" fallback="张" />);
    expect(screen.getByRole("img", { name: "张三" })).toBeInTheDocument();
    expect(screen.getByText("张")).toBeInTheDocument();
  });
});

describe("Kbd", () => {
  it("renders a kbd element", () => {
    const { container } = render(<Kbd>P</Kbd>);
    expect(container.querySelector("kbd")).toHaveTextContent("P");
  });
});

describe("Pagination", () => {
  it("is hidden for a single page", () => {
    const { container } = render(<Pagination page={1} total={1} onChange={() => {}} />);
    expect(container.querySelector(".pagination")).toBeNull();
  });

  it("disables the previous button on the first page", () => {
    render(<Pagination page={1} total={3} onChange={() => {}} />);
    expect(screen.getByRole("button", { name: "上一页" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "下一页" })).toBeEnabled();
  });
});

describe("NumberField", () => {
  it("clamps to the range", () => {
    const onChange = vi.fn();
    render(
      <NumberField value={1} minValue={1} maxValue={5} onChange={onChange}>
        <NumberField.Group>
          <NumberField.DecrementButton />
          <NumberField.Input />
          <NumberField.IncrementButton />
        </NumberField.Group>
      </NumberField>,
    );
    // 已到下限：减号禁用，加号可用。
    expect(screen.getByRole("button", { name: "减少" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "增加" })).toBeEnabled();
    screen.getByRole("button", { name: "增加" }).click();
    expect(onChange).toHaveBeenCalledWith(2);
  });
});

describe("toast", () => {
  it("exposes the notification methods", () => {
    for (const method of ["success", "danger", "warning", "info", "default", "clear"] as const) {
      expect(typeof toast[method]).toBe("function");
    }
  });

  it("can be spied on for notifications", () => {
    const spy = vi.spyOn(toast, "success");
    toast.success("已保存");
    expect(spy).toHaveBeenCalledWith("已保存");
    spy.mockRestore();
  });

  it("renders a native snackbar with the message and description", () => {
    toast.success("已保存", { description: "共 3 项" });
    const host = document.querySelector("m-snackbar")!;
    expect(host).not.toBeNull();
    // 正文由 Snackbar 自己在 top layer 的表面上渲染，宿主只承载配置属性。
    expect(host.getAttribute("message")).toBe("已保存 · 共 3 项");
    expect(document.querySelector(".mtrl-snackbar__text")?.textContent).toBe("已保存 · 共 3 项");
  });

  it("announces errors as an alert and the rest politely", () => {
    const lastSurface = () => {
      const found = document.querySelectorAll(".mtrl-snackbar");
      return found[found.length - 1];
    };

    toast.danger("保存失败");
    expect(lastSurface().getAttribute("role")).toBe("alert");

    resetToasts();
    toast.info("已同步");
    const info = lastSurface();
    expect(info.getAttribute("role")).toBeNull();
    expect(info.getAttribute("aria-live")).toBe("polite");
  });
});
