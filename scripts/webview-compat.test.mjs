// @vitest-environment node
import { describe, expect, it } from "vite-plus/test";
import postcss from "postcss";
import { readFileSync, readdirSync } from "node:fs";
import { compatibilityCss, compatibilityPostcssPlugins } from "./webview-compat.ts";
import { SHEET_METADATA } from "../src/compat/contracts.ts";
import selectorParser from "postcss-selector-parser";
import { selectorSpecificity } from "@csstools/selector-specificity";

describe("compatibility CSS pipeline", () => {
  it("extracts valid subjects from negated and nested :has", async () => {
    const output = await postcss([compatibilityCss()]).process(
      "body:not(:has(.nav)){bottom:16px}.switch:where(:not(:has(.label))){gap:0}",
      { from: undefined },
    );
    let sheet;
    output.root.walkDecls(SHEET_METADATA, (declaration) => {
      sheet = JSON.parse(Buffer.from(declaration.value.slice(1, -1), "base64").toString());
    });
    expect(sheet.has.map((rule) => rule.candidate)).toEqual(["body", ".switch"]);
  });
  it("preserves :has specificity, including zero-specificity arguments", async () => {
    for (const selector of [".a:has(.b[data-value])", ".a:has(:where(.b))", ".a:has(#id .b)"]) {
      const output = await postcss([compatibilityCss()]).process(`${selector}{color:red}`, {
        from: undefined,
      });
      let fallback;
      output.root.walkRules((rule) => {
        if (rule.selector.includes("data-szu-has-")) fallback = rule.selector;
      });
      expect(selectorSpecificity(selectorParser().astSync(fallback).first)).toEqual(
        selectorSpecificity(selectorParser().astSync(selector).first),
      );
    }
  });
  it("does not turn transform:none into a containing block", async () => {
    const output = await postcss([compatibilityCss()]).process(
      ".parent{transform:none}.child{scale:.8}",
      { from: undefined },
    );
    const transforms = [];
    output.root.walkRules((rule) => {
      if (rule.selector === ".parent")
        rule.walkDecls("transform", (declaration) => transforms.push(declaration.value));
    });
    expect(transforms).toEqual(["none"]);
    expect(output.css).toContain(":where(*, ::before, ::after)");
  });
  it("retains native layers and only flattens them for old WebViews", async () => {
    const result = await postcss(compatibilityPostcssPlugins()).process(
      "@layer base {.control:not(:has(> input:disabled)){color:color-mix(in srgb,var(--mtrl-sys-color-primary) 10%,transparent)}}",
      { from: undefined },
    );
    expect(result.css).toContain("@layer base");
    expect(result.css).toContain("@supports not (selector(:has(*)))");
    expect(result.css).toContain(
      "color-mix(in srgb,var(--mtrl-sys-color-primary) 10%,transparent)",
    );
    expect(result.css).toContain(":not([data-szu-has-");
    expect(result.css).toContain("rgba(var(--szu-compat-rgb-primary), 0.1)");
    expect(result.css).toContain(SHEET_METADATA);
  });
  it("keeps every gated fallback outside @layer so old WebViews can apply it", async () => {
    const result = await postcss([compatibilityCss()]).process(
      "@media (min-width: 600px) {@layer utilities{.a{scale:.8}}}",
      { from: undefined },
    );
    const nested = [];
    result.root.walkAtRules("supports", (rule) => {
      for (let parent = rule.parent; parent && parent.type !== "root"; parent = parent.parent)
        if (parent.type === "atrule" && parent.name === "layer") nested.push(rule.params);
    });
    expect(nested).toEqual([]);
    expect(result.css).toContain("@layer utilities");
    // 原生规则 + 媒体查询内、层级外的一份展开副本。
    expect(result.css.match(/\.a\{scale:\.8\}/g)).toHaveLength(2);
  });
  it("compiles every shipped Material component style, including nested :has", async () => {
    for (const file of readdirSync("node_modules/material/dist/elements/css").filter((file) =>
      file.endsWith(".css"),
    )) {
      const input = readFileSync(`node_modules/material/dist/elements/css/${file}`, "utf8");
      const output = await postcss([compatibilityCss()]).process(input, { from: file });
      expect(output.css.length).toBeGreaterThan(0);
      if (input.includes(":has(")) expect(output.css).toContain("data-szu-has-");
    }
  });
  it("derives queries from container widths rather than viewport media", async () => {
    const output = await postcss([compatibilityCss()]).process(
      ".paper{container:paper / inline-size}@container paper (min-width: 600px){.row{display:flex}}",
      { from: undefined },
    );
    expect(output.css).toContain(":where([data-szu-container-");
    expect(output.css).toContain("not (container-type: inline-size)");
    expect(output.css).not.toContain("@media");
  });
});
