/** @vitest-environment node */
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { JSDOM, VirtualConsole } from "jsdom";
import networkFeePage from "./page.js";

const LOGIN = "https://self.szu.edu.cn/self/nav_login";
const TARGET = "https://self.szu.edu.cn/self/nav_operators";
const RETURN = "http://tauri.localhost/campus-payments";
const ACCOUNT = "001234";
const PASSWORD = "dummy-secret-only-for-offline-tests";
const FORM =
  '<form id="loginform" method="post" action="/self/LoginAction.action"><input id="account" name="account" type="text" required><input id="pass" name="password" type="password" required><button type="submit">登录</button></form>';
const pages = [];

function page({
  url = LOGIN,
  html = FORM,
  ready = "complete",
  domReadyEnd = 0,
  childFrame = false,
} = {}) {
  const errors = [];
  const virtualConsole = new VirtualConsole();
  virtualConsole.on("jsdomError", (error) => errors.push(error));
  const dom = new JSDOM(html, { url, runScripts: "outside-only", virtualConsole });
  pages.push(dom);
  const { window } = dom;
  const { document } = window;
  if (childFrame) dom.reconfigure({ windowTop: {} });
  Object.defineProperty(document, "readyState", { configurable: true, value: ready });
  Object.defineProperty(window.performance, "getEntriesByType", {
    configurable: true,
    value: () => [{ domContentLoadedEventEnd: domReadyEnd }],
  });
  // jsdom has no layout; the production script still checks hidden/display/visibility.
  window.Element.prototype.getClientRects = function () {
    return this.isConnected ? [{}] : [];
  };
  const submit = vi.fn((event) => event.preventDefault());
  const click = vi.fn();
  document.querySelectorAll("form").forEach((form) => form.addEventListener("submit", submit));
  document
    .querySelectorAll("button[type=submit], input[type=submit]")
    .forEach((button) => button.addEventListener("click", click));
  const evaluate = (operation = "probe", overrides = {}) =>
    JSON.parse(
      window.eval(
        "(" +
          networkFeePage.toString() +
          ")(" +
          JSON.stringify({
            operation,
            expectedUrl: operation === "probe" ? url : undefined,
            generation: "1",
            newDocumentToken: "document-1",
            returnUrl: RETURN,
            message: "正在打开网费官网",
            manual: false,
            ...overrides,
          }) +
          ")",
      ),
    );
  const fill = (overrides = {}) =>
    evaluate("submit", {
      documentToken: "document-1",
      account: ACCOUNT,
      password: PASSWORD,
      ...overrides,
    });
  const expectEmpty = () => {
    for (const input of document.querySelectorAll("#account, #pass")) expect(input.value).toBe("");
    expect(click).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
  };
  return { dom, window, document, errors, evaluate, fill, submit, click, expectEmpty };
}

afterEach(() => {
  vi.restoreAllMocks();
  pages.splice(0).forEach((dom) => dom.window.close());
});

describe("network-fee main document login guard (offline)", () => {
  it("removes only web navigation after native toolbar acknowledgement and cleans up its listener", () => {
    const p = page({ html: "<title>网费套餐</title>" + FORM });
    const bridge = { postMessage: vi.fn(), onmessage: null };
    p.window.SzuApplicationBack = bridge;
    p.evaluate();
    const flow = p.window.__szuNetworkFeeFlow;
    const shadow = flow.toolbar.shadowRoot;
    const back = shadow.querySelector("a");
    const title = shadow.querySelector("h2");
    expect(bridge.postMessage).toHaveBeenCalledWith("toolbar-query");
    expect(back.hidden).toBe(false);
    bridge.onmessage({ data: "native-toolbar-ready" });
    expect(back.hidden).toBe(true);
    expect(title.hidden).toBe(true);
    expect(flow.manualButton.hidden).toBe(false);
    expect(shadow.querySelector("section").getAttribute("aria-label")).toBe("网费服务状态");
    expect(flow.message.isConnected).toBe(true);
    flow.manualButton.click();
    expect(flow.manual).toBe(true);
    expect(back.hidden).toBe(true);
    expect(flow.message.textContent).toContain("已停止自动操作");
    flow.dispose();
    expect(bridge.onmessage).toBeNull();
  });
  it("hands an already logged-in portal to the user without querying or operating payment controls", () => {
    const p = page({
      url: TARGET,
      html: '<button id="recharge">充值</button><button id="package">修改套餐</button>',
    });
    const operation = vi.fn();
    p.document
      .querySelectorAll("button")
      .forEach((button) => button.addEventListener("click", operation));
    expect(p.evaluate().kind).toBe("portal");
    expect(p.fill().kind).toBe("invalidForm");
    expect(operation).not.toHaveBeenCalled();
    expect(p.window.localStorage.length).toBe(0);
    expect(p.window.sessionStorage.length).toBe(0);
  });

  it("inspects synchronously without putting credentials into the probe", () => {
    const p = page();
    expect(p.evaluate()).toEqual({ kind: "loginReady", documentToken: "document-1" });
    p.expectEmpty();
    expect(Object.keys(p.window.__szuNetworkFeeFlow)).not.toContain("password");
    expect(Object.keys(p.window.__szuNetworkFeeFlow)).not.toContain("account");
  });

  it.each([
    [RETURN, TARGET],
    [TARGET, LOGIN],
    [LOGIN, TARGET],
    ["https://other.test/", LOGIN],
  ])(
    "ignores a provisional native URL %s -> %s until its document actually commits",
    (url, expectedUrl) => {
      const p = page({ url });
      expect(p.evaluate("probe", { expectedUrl }).kind).toBe("stale");
      expect(p.window.__szuNetworkFeeFlow).toBeUndefined();
      expect(p.document.querySelector("#szu-network-fee-toolbar")).toBeNull();
      expect(p.fill().kind).toBe("stale");
      p.expectEmpty();
    },
  );

  it("requires a native document URL for every probe", () => {
    const p = page();
    expect(p.evaluate("probe", { expectedUrl: undefined }).kind).toBe("stale");
    expect(p.window.__szuNetworkFeeFlow).toBeUndefined();
    p.expectEmpty();
  });

  it.each(["nav_login", "./nav_login", "/self/nav_login", LOGIN])(
    "waits through the official anonymous redirect document instead of treating it as logged in: %s",
    (destination) => {
      const p = page({
        url: TARGET,
        html:
          '<!doctype html><head><script>\nwindow.location.href = "' +
          destination +
          '";\n</script></head><body></body>',
      });
      expect(p.evaluate().kind).toBe("pending");
      expect(p.evaluate().kind).toBe("pending");
      expect(p.window.__szuNetworkFeeFlow.submitted).toBe(false);
      expect(p.window.__szuNetworkFeeFlow.manual).toBe(false);
      expect(p.errors).toHaveLength(0);
      p.expectEmpty();
    },
  );

  it("accepts the official relative form action and input submit control without bypassing validation", () => {
    const p = page({
      html: FORM.replace(
        'action="/self/LoginAction.action"',
        'name="loginform" action="LoginAction.action"',
      ).replace(
        '<button type="submit">登录</button>',
        '<input type="submit" name="Submit" value="登 录">',
      ),
    });
    expect(p.evaluate().kind).toBe("loginReady");
    expect(p.fill().kind).toBe("submitted");
    expect(p.click).toHaveBeenCalledTimes(1);
    expect(p.submit).toHaveBeenCalledTimes(1);
  });

  it("covers the full viewport and makes the page inert from loading through submission", () => {
    const p = page({ ready: "loading" });
    expect(p.evaluate().kind).toBe("loading");
    const toolbar = p.document.getElementById("szu-network-fee-toolbar");
    expect(toolbar.style.position).toBe("fixed");
    expect(toolbar.style.inset).toBe("0px");
    expect(toolbar.dataset.mode).toBe("automatic");
    expect(toolbar.shadowRoot.querySelector("[role=dialog]").getAttribute("aria-modal")).toBe(
      "true",
    );
    expect(toolbar.shadowRoot.querySelector("button").textContent).toBe("手动操作");
    expect(p.document.body.hasAttribute("inert")).toBe(true);
    expect(p.document.documentElement.style.overflow).toBe("hidden");
    Object.defineProperty(p.document, "readyState", { value: "complete" });
    expect(p.fill().kind).toBe("submitted");
    expect(p.document.body.hasAttribute("inert")).toBe(true);
    expect(toolbar.dataset.mode).toBe("automatic");
  });

  it("does not wait for slow images or async resources after DOM-ready handlers have finished", () => {
    const p = page({ ready: "interactive", domReadyEnd: 12 });
    expect(p.evaluate().kind).toBe("loginReady");
    expect(p.fill().kind).toBe("submitted");
    expect(p.document.readyState).toBe("interactive");
    expect(p.submit).toHaveBeenCalledOnce();
  });

  it("does not confuse interactive parsing with completed DOM-ready handlers", () => {
    const p = page({ ready: "interactive" });
    expect(p.evaluate().kind).toBe("loading");
    expect(p.fill().kind).toBe("loading");
    p.expectEmpty();
  });

  it("waits until all DOM-ready handlers finish, including a later handler revealing captcha", async () => {
    const p = page({
      ready: "interactive",
      html: FORM + '<div id="tr_random" hidden>验证码</div>',
    });
    expect(p.evaluate().kind).toBe("loading");
    p.document.addEventListener("DOMContentLoaded", () => {
      p.document.querySelector("#tr_random").hidden = false;
    });
    p.document.dispatchEvent(new p.window.Event("DOMContentLoaded"));
    expect(p.evaluate().kind).toBe("loading");
    await new Promise((resolve) => p.window.setTimeout(resolve, 0));
    expect(p.evaluate().kind).toBe("captcha");
    p.expectEmpty();
  });

  it("can use its DOM-ready listener when navigation timing is unavailable", async () => {
    const p = page({ ready: "interactive" });
    Reflect.deleteProperty(p.window.performance, "getEntriesByType");
    expect(p.evaluate().kind).toBe("loading");
    p.document.dispatchEvent(new p.window.Event("DOMContentLoaded"));
    await new Promise((resolve) => p.window.setTimeout(resolve, 0));
    expect(p.evaluate().kind).toBe("loginReady");
    expect(p.fill().kind).toBe("submitted");
  });

  it.each([
    ['<div id="tr_random">验证码</div>', "captcha"],
    ['<div id="fielderror1">登录错误</div>', "loginError"],
  ])(
    "still respects initialized official validation while other resources load: %s",
    (html, kind) => {
      const p = page({ ready: "interactive", domReadyEnd: 12, html: FORM + html });
      expect(p.evaluate().kind).toBe(kind);
      expect(p.fill().kind).toBe(kind);
      p.expectEmpty();
    },
  );

  it.each([false, true])(
    "keeps the back arrow, current title and status in a fixed top bar (manual=%s)",
    (manual) => {
      const p = page({ html: "<title>网费套餐预订</title>" + FORM });
      p.evaluate();
      if (manual) p.evaluate("toolbar", { manual: true, message: "请在官网自行办理" });
      const toolbar = p.window.__szuNetworkFeeFlow.toolbar;
      const shadow = toolbar.shadowRoot;
      expect(toolbar.style.position).toBe("fixed");
      // jsdom retains inset as a shorthand; native layout is checked separately.
      expect(toolbar.style.inset).toBe(manual ? "0px 0px auto" : "0px");
      const title = shadow.querySelector("h2");
      expect(title.textContent).toBe("网费套餐预订");
      expect(title.hidden).toBe(false);
      expect(shadow.querySelector("section").getAttribute("aria-labelledby")).toBe(title.id);
      const back = shadow.querySelector("a");
      expect(back.getAttribute("aria-label")).toBe("返回校园卡/电费/网费");
      expect(back.title).toBe("返回校园卡/电费/网费");
      expect(back.href).toBe(RETURN);
      expect(back.querySelector("svg").getAttribute("aria-hidden")).toBe("true");
      expect(back.querySelector("path").getAttribute("d")).toBe("m12 19-7-7 7-7M5 12h14");
      expect(shadow.querySelector("section").firstElementChild).toBe(back);
      expect(back.nextElementSibling).toBe(title);
      expect(shadow.querySelector("[role=status]").textContent).toBe(
        manual ? "请在官网自行办理" : "正在打开网费官网",
      );
      const css = shadow.querySelector("style").textContent;
      expect(css).toContain("grid-template-columns:44px minmax(0,1fr) auto");
      expect(css).toContain("text-overflow:ellipsis");
      expect(css).toContain("@media(max-width:560px)");
      expect(css).toContain("env(safe-area-inset-top,0px)");
    },
  );

  it("uses a safe fallback and follows title changes after manual handoff without further probes", async () => {
    const p = page();
    p.evaluate();
    const flow = p.window.__szuNetworkFeeFlow;
    const title = flow.toolbar.shadowRoot.querySelector("h2");
    expect(title.textContent).toBe("网费服务");
    flow.manualButton.click();
    const manualMessage = flow.message.textContent;
    const hostileTitle = '<img src=x onerror="window.titleExecuted=true"> & 套餐';
    p.document.title = hostileTitle;
    await Promise.resolve();
    expect(title.textContent).toBe(hostileTitle);
    expect(title.title).toBe(hostileTitle);
    expect(title.children).toHaveLength(0);
    expect(p.window.titleExecuted).toBeUndefined();
    expect(flow.message.textContent).toBe(manualMessage);
    expect(p.document.body.hasAttribute("inert")).toBe(false);
    p.document.title = "  ";
    await Promise.resolve();
    expect(title.textContent).toBe("网费服务");
    p.expectEmpty();
  });

  it("tracks titles when the head is inserted late or replaced", async () => {
    const p = page({ ready: "loading" });
    p.document.head.remove();
    p.evaluate();
    const title = p.window.__szuNetworkFeeFlow.toolbar.shadowRoot.querySelector("h2");
    expect(title.textContent).toBe("网费服务");
    for (const text of ["网费登录", "套餐页面"]) {
      const head = p.document.createElement("head");
      const pageTitle = p.document.createElement("title");
      pageTitle.textContent = text;
      head.append(pageTitle);
      if (p.document.head) p.document.head.replaceWith(head);
      else p.document.documentElement.prepend(head);
      await Promise.resolve();
      expect(title.textContent).toBe(text);
      pageTitle.firstChild.data = text + " · 已更新";
      await Promise.resolve();
      expect(title.textContent).toBe(text + " · 已更新");
    }
  });

  it("reserves the measured bar height, follows resizing and restores the original padding", () => {
    const p = page();
    const style = p.document.body.style;
    style.setProperty("padding", "16px 24px", "important");
    const writePadding = vi.spyOn(style, "setProperty");
    const bounds = vi
      .spyOn(p.window.HTMLElement.prototype, "getBoundingClientRect")
      .mockReturnValue({ height: 112 });
    p.evaluate();
    const initialPadding = style.paddingTop;
    expect(initialPadding).toBe("calc(128px)");
    expect(style.paddingRight).toBe("24px");
    // jsdom drops a calc longhand priority when expanding a shorthand. Check
    // the native CSSOM write here; real-browser QA checks the computed padding.
    expect(writePadding).toHaveBeenCalledWith("padding-top", "calc(16px + 112px)", "important");
    p.evaluate();
    expect(style.paddingTop).toBe(initialPadding);
    bounds.mockReturnValue({ height: 84 });
    p.window.__szuNetworkFeeFlow.manualButton.click();
    expect(style.paddingTop).toBe("calc(100px)");
    bounds.mockReturnValue({ height: 104 });
    p.window.dispatchEvent(new p.window.Event("resize"));
    expect(style.paddingTop).toBe("calc(120px)");
    p.evaluate("cancel");
    expect(style.paddingTop).toBe("16px");
    expect(style.paddingRight).toBe("24px");
    expect(style.paddingBottom).toBe("16px");
    expect(style.paddingLeft).toBe("24px");
    expect(style.getPropertyPriority("padding-top")).toBe("important");
    p.window.dispatchEvent(new p.window.Event("resize"));
    expect(style.paddingTop).toBe("16px");
  });

  it("reserves space for a late or replacement body even after manual takeover", async () => {
    const p = page({ ready: "loading" });
    p.document.body.remove();
    vi.spyOn(p.window.HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      height: 80,
    });
    p.evaluate();
    p.window.__szuNetworkFeeFlow.manualButton.click();
    const firstBody = p.document.createElement("body");
    p.document.documentElement.append(firstBody);
    await Promise.resolve();
    expect(firstBody.style.paddingTop).toBe("calc(80px)");
    expect(firstBody.hasAttribute("inert")).toBe(false);
    const nextBody = p.document.createElement("body");
    nextBody.style.paddingTop = "10px";
    firstBody.replaceWith(nextBody);
    await Promise.resolve();
    expect(firstBody.style.paddingTop).toBe("");
    expect(nextBody.style.paddingTop).toBe("calc(90px)");
    expect(nextBody.hasAttribute("inert")).toBe(false);
    p.evaluate("cancel");
    expect(nextBody.style.paddingTop).toBe("10px");
    expect(nextBody.style.getPropertyPriority("padding-top")).toBe("");
  });

  it("does not accumulate reserved space when a new task takes over", () => {
    const p = page();
    vi.spyOn(p.window.HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      height: 100,
    });
    p.evaluate();
    expect(p.document.body.style.paddingTop).toBe("calc(100px)");
    p.evaluate("probe", { generation: "2", newDocumentToken: "document-2" });
    expect(p.document.body.style.paddingTop).toBe("calc(100px)");
    p.evaluate("cancel", { generation: "2" });
    expect(p.document.body.style.paddingTop).toBe("");
  });

  it.each(["cancel", "pagehide"])(
    "stops title and layout observers on %s, including queued resize callbacks",
    async (operation) => {
      const p = page({ html: "<title>官网标题</title>" + FORM });
      let notifyResize;
      const observe = vi.fn();
      const disconnect = vi.fn();
      p.window.ResizeObserver = class {
        constructor(callback) {
          notifyResize = callback;
        }
        observe = observe;
        disconnect = disconnect;
      };
      const bounds = vi
        .spyOn(p.window.HTMLElement.prototype, "getBoundingClientRect")
        .mockReturnValue({ height: 100 });
      p.evaluate();
      const flow = p.window.__szuNetworkFeeFlow;
      const title = flow.toolbar.shadowRoot.querySelector("h2");
      expect(observe).toHaveBeenCalledWith(flow.toolbar.shadowRoot.querySelector("section"));
      bounds.mockReturnValue({ height: 124 });
      notifyResize();
      expect(p.document.body.style.paddingTop).toBe("calc(124px)");
      if (operation === "cancel") p.evaluate("cancel");
      else p.window.dispatchEvent(new p.window.Event("pagehide"));
      expect(disconnect).toHaveBeenCalledOnce();
      p.document.title = "不得再同步的标题";
      notifyResize();
      await Promise.resolve();
      expect(title.textContent).toBe("官网标题");
      expect(p.document.body.style.paddingTop).toBe("");
      expect(flow.toolbar.isConnected).toBe(false);
      p.expectEmpty();
    },
  );

  it("releases the overlay only at handoff, and never re-locks a manually controlled document", () => {
    const p = page();
    p.evaluate();
    expect(p.document.body.hasAttribute("inert")).toBe(true);
    p.evaluate("toolbar", { manual: true, message: "已交给用户办理" });
    expect(p.document.body.hasAttribute("inert")).toBe(false);
    expect(p.document.documentElement.style.overflow).toBe("");
    const toolbar = p.window.__szuNetworkFeeFlow.toolbar;
    expect(toolbar.dataset.mode).toBe("manual");
    expect(toolbar.shadowRoot.querySelector("[aria-modal]")).toBeNull();
    expect(toolbar.shadowRoot.querySelector("[role=status]").textContent).toBe("已交给用户办理");
    expect(p.evaluate().kind).toBe("manual");
    expect(p.document.body.hasAttribute("inert")).toBe(false);
    expect(toolbar.shadowRoot.querySelector("[role=status]").textContent).toBe("已交给用户办理");
    expect(p.fill().kind).toBe("manual");
    p.expectEmpty();
  });

  it.each(["cancel", "pagehide"])(
    "releases page locks when %s invalidates the document",
    (operation) => {
      const p = page();
      p.evaluate();
      if (operation === "cancel") p.evaluate("cancel");
      else p.window.dispatchEvent(new p.window.Event("pagehide"));
      expect(p.document.body.hasAttribute("inert")).toBe(false);
      expect(p.document.documentElement.style.overflow).toBe("");
      expect(p.fill().kind).toBe("stale");
      p.expectEmpty();
    },
  );

  it("preserves pre-existing inert and overflow settings after explicit manual takeover", () => {
    const p = page();
    p.document.body.setAttribute("inert", "original");
    p.document.documentElement.style.setProperty("overflow", "clip", "important");
    p.evaluate();
    p.window.__szuNetworkFeeFlow.manualButton.click();
    expect(p.document.body.getAttribute("inert")).toBe("original");
    expect(p.document.documentElement.style.overflow).toBe("clip");
    expect(p.document.documentElement.style.getPropertyPriority("overflow")).toBe("important");
    expect(p.window.__szuNetworkFeeFlow.toolbar.dataset.mode).toBe("manual");
  });

  it("mounts before the body exists and locks a body inserted later", async () => {
    const p = page({ ready: "loading" });
    p.document.body.remove();
    expect(p.evaluate().kind).toBe("loading");
    expect(p.window.__szuNetworkFeeFlow.toolbar.parentElement).toBe(p.document.documentElement);
    const body = p.document.createElement("body");
    p.document.documentElement.append(body);
    await Promise.resolve();
    expect(body.hasAttribute("inert")).toBe(true);
    p.window.__szuNetworkFeeFlow.manualButton.click();
    expect(body.hasAttribute("inert")).toBe(false);
  });

  it("traps keyboard focus in the overlay without making Escape an implicit takeover", () => {
    const p = page();
    const handlers = new Map();
    const add = p.document.addEventListener.bind(p.document);
    vi.spyOn(p.document, "addEventListener").mockImplementation((type, listener, options) => {
      handlers.set(type, listener);
      add(type, listener, options);
    });
    p.evaluate();
    const toolbar = p.window.__szuNetworkFeeFlow.toolbar;
    const shadow = toolbar.shadowRoot;
    const back = shadow.querySelector("a");
    const manual = shadow.querySelector("button");
    const key = (key, shiftKey = false) => {
      const event = {
        type: "keydown",
        key,
        shiftKey,
        isTrusted: true,
        target: toolbar,
        preventDefault: vi.fn(),
        stopImmediatePropagation: vi.fn(),
      };
      handlers.get("keydown")(event);
      expect(event.preventDefault).toHaveBeenCalledOnce();
      expect(event.stopImmediatePropagation).toHaveBeenCalledOnce();
    };
    key("Tab");
    expect(shadow.activeElement).toBe(back);
    key("Tab");
    expect(shadow.activeElement).toBe(manual);
    key("Tab");
    expect(shadow.activeElement).toBe(back);
    key("Tab", true);
    expect(shadow.activeElement).toBe(manual);
    key("Escape");
    expect(p.window.__szuNetworkFeeFlow.manual).toBe(false);
    expect(p.document.body.hasAttribute("inert")).toBe(true);
    expect(handlers.has("submit")).toBe(false);
    p.expectEmpty();
  });

  it("keeps explicit manual copy and no longer blocks input after runtime acknowledgment", () => {
    const p = page();
    const handlers = new Map();
    const add = p.document.addEventListener.bind(p.document);
    vi.spyOn(p.document, "addEventListener").mockImplementation((type, listener, options) => {
      handlers.set(type, listener);
      add(type, listener, options);
    });
    p.evaluate();
    const flow = p.window.__szuNetworkFeeFlow;
    flow.manualButton.click();
    const manualMessage = flow.message.textContent;
    p.evaluate("toolbar", { manual: true, message: "runtime fallback copy" });
    p.evaluate();
    expect(flow.message.textContent).toBe(manualMessage);
    const event = {
      type: "keydown",
      key: "a",
      isTrusted: true,
      target: p.document.querySelector("#account"),
      preventDefault: vi.fn(),
      stopImmediatePropagation: vi.fn(),
    };
    handlers.get("keydown")(event);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(event.stopImmediatePropagation).not.toHaveBeenCalled();
    expect(p.fill().kind).toBe("manual");
    expect(p.evaluate("return").kind).toBe("manual");
    p.expectEmpty();
  });

  it("preserves overflow longhands and focus when the overlay is released", () => {
    const p = page();
    const account = p.document.querySelector("#account");
    account.focus();
    const style = p.document.documentElement.style;
    style.setProperty("overflow-x", "clip", "important");
    style.setProperty("overflow-y", "scroll");
    p.evaluate();
    p.window.__szuNetworkFeeFlow.manualButton.click();
    expect(style.getPropertyValue("overflow-x")).toBe("clip");
    expect(style.getPropertyPriority("overflow-x")).toBe("important");
    expect(style.getPropertyValue("overflow-y")).toBe("scroll");
    expect(style.getPropertyPriority("overflow-y")).toBe("");
    expect(p.document.activeElement).toBe(account);
  });

  it("moves the page lock when body is replaced, then disconnects it on cancellation", async () => {
    const p = page();
    const oldBody = p.document.body;
    p.evaluate();
    const body = p.document.createElement("body");
    oldBody.replaceWith(body);
    await Promise.resolve();
    expect(oldBody.hasAttribute("inert")).toBe(false);
    expect(body.hasAttribute("inert")).toBe(true);
    p.evaluate("cancel");
    expect(body.hasAttribute("inert")).toBe(false);
    const unlockedBody = p.document.createElement("body");
    body.replaceWith(unlockedBody);
    await Promise.resolve();
    expect(unlockedBody.hasAttribute("inert")).toBe(false);
    expect(p.document.getElementById("szu-network-fee-toolbar")).toBeNull();
  });

  it("cleans old listeners and page locks before a new generation takes ownership", () => {
    const p = page();
    p.document.documentElement.style.overflow = "clip";
    const remove = vi.spyOn(p.document, "removeEventListener");
    p.evaluate();
    const previous = p.window.__szuNetworkFeeFlow;
    p.evaluate("probe", { generation: "2", newDocumentToken: "document-2" });
    expect(remove).toHaveBeenCalledWith("keydown", expect.any(Function), true);
    expect(previous.cancelled).toBe(true);
    expect(previous.toolbar.isConnected).toBe(false);
    expect(p.document.body.hasAttribute("inert")).toBe(true);
    p.window.__szuNetworkFeeFlow.manualButton.click();
    expect(p.document.body.hasAttribute("inert")).toBe(false);
    expect(p.document.documentElement.style.overflow).toBe("clip");
  });

  it("does not install persistent callbacks from the credential-bearing invocation", () => {
    const p = page();
    p.evaluate();
    const documentListener = vi.spyOn(p.document, "addEventListener");
    const windowListener = vi.spyOn(p.window, "addEventListener");
    const timer = vi.spyOn(p.window, "setTimeout");
    const observation = vi.spyOn(p.window.MutationObserver.prototype, "observe");
    const contextKeys = Object.keys(p.window.__szuNetworkFeeFlow);
    expect(p.fill().kind).toBe("submitted");
    expect(documentListener).not.toHaveBeenCalled();
    expect(windowListener).not.toHaveBeenCalled();
    expect(timer).not.toHaveBeenCalled();
    expect(observation).not.toHaveBeenCalled();
    expect(Object.keys(p.window.__szuNetworkFeeFlow)).toEqual(contextKeys);
    expect(JSON.stringify(p.window.__szuNetworkFeeFlow)).not.toContain(PASSWORD);
  });

  it("uses completed legacy DOM-ready timing when modern navigation timing is absent", () => {
    const p = page({ ready: "interactive" });
    Reflect.deleteProperty(p.window.performance, "getEntriesByType");
    Object.defineProperty(p.window.performance, "timing", {
      value: { domContentLoadedEventEnd: 12 },
    });
    expect(p.evaluate().kind).toBe("loginReady");
    expect(p.fill().kind).toBe("submitted");
  });

  it("cancels a queued DOM-ready task when the document is invalidated", async () => {
    const p = page({ ready: "interactive" });
    p.evaluate();
    p.document.dispatchEvent(new p.window.Event("DOMContentLoaded"));
    const previous = p.window.__szuNetworkFeeFlow;
    p.evaluate("cancel");
    await new Promise((resolve) => p.window.setTimeout(resolve, 0));
    expect(previous.domReady).toBe(false);
    expect(p.fill().kind).toBe("stale");
    p.expectEmpty();
  });

  it("uses native setters, input/change events and the official submit button only once", () => {
    const p = page();
    const events = [];
    for (const input of p.document.querySelectorAll("input")) {
      for (const type of ["input", "change"])
        input.addEventListener(type, (event) => {
          events.push(input.id + ":" + event.type);
          expect(event.bubbles).toBe(true);
          expect(event.isTrusted).toBe(false);
        });
    }
    p.document.querySelector("button").addEventListener("click", () => events.push("click"));
    p.evaluate();
    expect(p.fill().kind).toBe("submitted");
    expect(events).toEqual([
      "account:input",
      "account:change",
      "pass:input",
      "pass:change",
      "click",
    ]);
    expect(p.document.querySelector("#account").value).toBe(ACCOUNT);
    expect(p.document.querySelector("#pass").value).toBe(PASSWORD);
    expect(p.fill({ password: "must-not-overwrite" }).kind).toBe("submitted");
    expect(p.evaluate().kind).toBe("submitted");
    expect(p.document.querySelector("#pass").value).toBe(PASSWORD);
    expect(p.click).toHaveBeenCalledTimes(1);
    expect(p.submit).toHaveBeenCalledTimes(1);
    expect(p.window.__szuNetworkFeeFlow.manual).toBe(false);
    expect(JSON.stringify(p.window.__szuNetworkFeeFlow)).not.toContain(PASSWORD);
    expect(p.window.location.href).toBe(LOGIN);
    expect(p.window.localStorage.length + p.window.sessionStorage.length).toBe(0);
  });

  it("preserves native form validation rather than calling form.submit()", () => {
    const p = page({
      html: FORM.replace("</form>", '<input name="native-required" required></form>'),
    });
    p.evaluate();
    expect(p.fill().kind).toBe("submitted");
    expect(p.click).toHaveBeenCalledTimes(1);
    expect(p.submit).not.toHaveBeenCalled();
    expect(p.document.querySelector("form").checkValidity()).toBe(false);
    expect(p.fill().kind).toBe("submitted");
    expect(p.click).toHaveBeenCalledTimes(1);
  });

  it("safely injects quotes and backslashes as data, never executable JavaScript", () => {
    const p = page();
    const password = "dummy-\"quote'\\; window.unwantedExecution = true; //";
    p.evaluate();
    expect(p.fill({ password }).kind).toBe("submitted");
    expect(p.document.querySelector("#pass").value).toBe(password);
    expect(p.window.unwantedExecution).toBeUndefined();
    expect(p.submit).toHaveBeenCalledTimes(1);
  });

  it.each(["line\nbreak", "carriage\rreturn", "both\r\nbreaks"])(
    "fails closed if a native password input sanitizes line breaks: %j",
    (password) => {
      const p = page();
      p.evaluate();
      expect(p.fill({ password }).kind).toBe("invalidForm");
      p.expectEmpty();
      expect(p.window.__szuNetworkFeeFlow.manual).toBe(true);
    },
  );

  it.each([
    "http://self.szu.edu.cn/self/nav_login",
    "https://self.szu.edu.cn.evil.test/self/nav_login",
    "https://self.szu.edu.cn@evil.test/self/nav_login",
    "https://user:password@self.szu.edu.cn/self/nav_login",
    "https://other.szu.edu.cn/self/nav_login",
    "https://self.szu.edu.cn:444/self/nav_login",
    "https://self.szu.edu.cn/self/nav_login/",
    "https://self.szu.edu.cn/self/nav_login?next=other",
    "https://self.szu.edu.cn/self/nav_login#fragment",
    "https://self.szu.edu.cn/other/nav_login",
    TARGET,
  ])("does not fill a form at an unapproved URL: %s", (url) => {
    const p = page({ url });
    expect(["other", "invalidForm"]).toContain(p.evaluate().kind);
    expect(p.fill().kind).toBe("invalidForm");
    p.expectEmpty();
  });

  it("rejects subframes even when their apparent URL and form match", () => {
    const p = page({ childFrame: true });
    expect(p.evaluate().kind).toBe("other");
    expect(p.fill().kind).toBe("other");
    expect(p.window.__szuNetworkFeeFlow).toBeUndefined();
    expect(p.document.querySelector("#szu-network-fee-toolbar")).toBeNull();
    p.expectEmpty();
  });

  it.each([
    [
      "cross-origin action",
      (form) => form.setAttribute("action", "https://evil.test/self/LoginAction.action"),
    ],
    [
      "insecure action",
      (form) => form.setAttribute("action", "http://self.szu.edu.cn/self/LoginAction.action"),
    ],
    ["wrong action", (form) => form.setAttribute("action", "/self/Other.action")],
    ["action query", (form) => form.setAttribute("action", "/self/LoginAction.action?next=evil")],
    [
      "action credentials",
      (form) =>
        form.setAttribute("action", "https://user:secret@self.szu.edu.cn/self/LoginAction.action"),
    ],
    ["GET action", (form) => form.setAttribute("method", "get")],
    ["missing action", (form) => form.removeAttribute("action")],
    ["another frame", (form) => form.setAttribute("target", "_blank")],
    ["account type", (form) => (form.querySelector("#account").type = "email")],
    ["password type", (form) => (form.querySelector("#pass").type = "text")],
    ["missing password", (form) => form.querySelector("#pass").remove()],
    ["wrong field name", (form) => (form.querySelector("#pass").name = "other")],
    ["disabled field", (form) => (form.querySelector("#pass").disabled = true)],
    ["readonly field", (form) => (form.querySelector("#pass").readOnly = true)],
    ["hidden field", (form) => (form.querySelector("#pass").style.display = "none")],
    ["hidden form", (form) => (form.style.visibility = "hidden")],
    ["duplicate account", (form) => form.appendChild(form.querySelector("#account").cloneNode())],
    ["duplicate submit", (form) => form.appendChild(form.querySelector("button").cloneNode())],
    ["different owner", (form) => form.querySelector("#pass").setAttribute("form", "other")],
    ...["formaction", "formmethod", "formtarget", "formenctype", "formnovalidate"].map(
      (attribute) => [
        attribute + " override",
        (form) => form.querySelector("button").setAttribute(attribute, "override"),
      ],
    ),
  ])("rejects anomalous forms before filling: %s", (_, change) => {
    const p = page();
    change(p.document.querySelector("form"));
    expect(p.evaluate().kind).toBe("invalidForm");
    expect(p.fill().kind).toBe("invalidForm");
    p.expectEmpty();
  });

  it("rejects relative actions redirected by a cross-origin base element", () => {
    const p = page({ html: '<base href="https://evil.test/">' + FORM });
    expect(p.evaluate().kind).toBe("invalidForm");
    expect(p.fill().kind).toBe("invalidForm");
    p.expectEmpty();
  });

  it.each([FORM + FORM, '<div id="loginform"></div>'])(
    "rejects duplicate or non-form login IDs",
    (html) => {
      const p = page({ html });
      expect(p.evaluate().kind).toBe("invalidForm");
      expect(p.fill().kind).toBe("invalidForm");
      p.expectEmpty();
    },
  );

  it("reports missing / unfinished forms for bounded Rust polling, without submitting", () => {
    const p = page({ html: "<p>Loading</p>" });
    expect(p.evaluate().kind).toBe("pending");
    expect(p.fill().kind).toBe("pending");
    const loading = page({ ready: "loading" });
    expect(loading.evaluate().kind).toBe("loading");
    expect(loading.fill().kind).toBe("loading");
    loading.expectEmpty();
  });

  it.each([
    '<div id="tr_random">验证码</div>',
    '<input id="radomcode">',
    '<img id="codeimage">',
    '<input name="CAPTCHA">',
  ])("leaves visible captcha to the user: %s", (captcha) => {
    const p = page({ html: FORM + captcha });
    expect(p.evaluate().kind).toBe("captcha");
    expect(p.fill().kind).toBe("captcha");
    p.expectEmpty();
  });

  it("recognizes captcha even before the rest of the form exists", () => {
    const p = page({ html: '<div id="tr_random">验证码</div>' });
    expect(p.evaluate().kind).toBe("captcha");
    p.expectEmpty();
  });

  it.each(["hidden", 'style="display:none"', 'style="visibility:hidden"'])(
    "does not mistake a hidden captcha template for a challenge: %s",
    (attribute) => {
      const p = page({
        html: FORM + "<div " + attribute + '><input id="radomcode"><img id="codeimage"></div>',
      });
      expect(p.evaluate().kind).toBe("loginReady");
      expect(p.fill().kind).toBe("submitted");
    },
  );

  it.each(["fielderror1", "fielderror2"])(
    "does not refill or retry when the website reports an error in %s",
    (id) => {
      const p = page({ html: FORM + '<div id="' + id + '">账号或密码错误</div>' });
      expect(p.evaluate().kind).toBe("loginError");
      expect(p.fill().kind).toBe("loginError");
      p.expectEmpty();
    },
  );

  it.each([
    ["destination", (p) => (p.document.querySelector("form").action = "https://evil.test/")],
    ["form replacement", (p) => (p.document.querySelector("form").outerHTML = FORM)],
    ["navigation", (p) => p.dom.reconfigure({ url: "https://evil.test/" })],
    [
      "captcha",
      (p) =>
        p.document.querySelector("form").insertAdjacentHTML("beforeend", '<input name="captcha">'),
    ],
  ])(
    "revalidates after input handlers change the %s, clears inserted values, and does not click",
    (change, mutate) => {
      const p = page();
      const account = p.document.querySelector("#account");
      const password = p.document.querySelector("#pass");
      account.addEventListener("input", () => mutate(p));
      p.evaluate();
      expect(p.fill().kind).toBe(change === "captcha" ? "captcha" : "invalidForm");
      p.expectEmpty();
      expect(account.value).toBe("");
      expect(password.value).toBe("");
    },
  );

  it.each(["account", "pass"])("does not overwrite a user/password-manager supplied %s", (id) => {
    const p = page();
    p.document.querySelector("#" + id).value = "already-entered";
    expect(p.evaluate().kind).toBe("manual");
    expect(p.fill().kind).toBe("manual");
    expect(p.document.querySelector("#" + id).value).toBe("already-entered");
    expect(p.click).not.toHaveBeenCalled();
  });

  it.each(["pointerdown", "click", "keydown", "beforeinput", "touchstart", "wheel"])(
    "blocks trusted %s outside the overlay without implicitly switching to manual",
    (type) => {
      const p = page();
      const handlers = new Map();
      const add = p.document.addEventListener.bind(p.document);
      vi.spyOn(p.document, "addEventListener").mockImplementation((type, listener, options) => {
        handlers.set(type, listener);
        add(type, listener, options);
      });
      expect(p.evaluate().kind).toBe("loginReady");
      // jsdom cannot create trusted input. Exercise the registered capture guard;
      // the native-browser fixture additionally checks actual mouse/keyboard input.
      const event = {
        isTrusted: true,
        target: p.document.querySelector("#account"),
        preventDefault: vi.fn(),
        stopImmediatePropagation: vi.fn(),
      };
      handlers.get(type)(event);
      expect(event.preventDefault).toHaveBeenCalledOnce();
      expect(event.stopImmediatePropagation).toHaveBeenCalledOnce();
      expect(p.window.__szuNetworkFeeFlow.manual).toBe(false);
      p.expectEmpty();
      expect(p.fill().kind).toBe("submitted");
      expect(p.submit).toHaveBeenCalledOnce();
    },
  );

  it("manual takeover synchronously blocks queued submissions and automatic restoration", () => {
    const p = page();
    p.evaluate();
    const manual = p.document.querySelector("aside").shadowRoot.querySelector("button");
    manual.click();
    expect(manual.hidden).toBe(true);
    expect(p.document.body.hasAttribute("inert")).toBe(false);
    expect(p.window.__szuNetworkFeeFlow.toolbar.dataset.mode).toBe("manual");
    expect(p.fill().kind).toBe("manual");
    expect(p.evaluate("restore", { documentToken: "document-1" }).kind).toBe("manual");
    p.expectEmpty();
    expect(p.errors).toHaveLength(0);
  });

  it("installs one fixed local return link, whose click invalidates queued callbacks immediately", () => {
    const p = page();
    p.evaluate();
    p.evaluate();
    expect(p.document.querySelectorAll("#szu-network-fee-toolbar")).toHaveLength(1);
    const link = p.document.querySelector("aside").shadowRoot.querySelector("a");
    expect(link.getAttribute("aria-label")).toBe("返回校园卡/电费/网费");
    expect(link.href).toBe(RETURN);
    expect(link.target).toBe("_self");
    link.addEventListener("click", (event) => event.preventDefault());
    link.click();
    expect(p.fill().kind).toBe("stale");
    expect(p.evaluate().kind).toBe("stale");
    p.expectEmpty();
  });

  it("requires an existing matching inspected document and generation", () => {
    const p = page();
    expect(p.fill().kind).toBe("stale");
    expect(p.window.__szuNetworkFeeFlow).toBeUndefined();
    p.evaluate();
    expect(p.fill({ generation: "2" }).kind).toBe("stale");
    expect(p.fill({ documentToken: "old-document" }).kind).toBe("stale");
    p.expectEmpty();
  });

  it("rejects old probes and cancellation after a newer task took ownership", () => {
    const p = page();
    p.evaluate("probe", { generation: "2", newDocumentToken: "document-2" });
    expect(p.evaluate().kind).toBe("stale");
    expect(p.evaluate("cancel").kind).toBe("stale");
    expect(p.window.__szuNetworkFeeFlow.generation).toBe("2");
    expect(p.window.__szuNetworkFeeFlow.cancelled).toBe(false);
    expect(p.fill().kind).toBe("stale");
    p.expectEmpty();
    expect(p.fill({ generation: "2", documentToken: "document-2" }).kind).toBe("submitted");
  });

  it.each(["cancel", "pagehide"])(
    "invalidates the document on %s so late operations cannot fill",
    (mode) => {
      const p = page();
      p.evaluate();
      if (mode === "cancel") p.evaluate("cancel");
      else p.window.dispatchEvent(new p.window.Event("pagehide"));
      expect(p.fill().kind).toBe("stale");
      expect(p.evaluate().kind).toBe("stale");
      p.expectEmpty();
    },
  );

  it("restores only the fixed target once, after the login form is gone", () => {
    const p = page();
    p.evaluate();
    expect(p.evaluate("restore", { documentToken: "document-1" }).kind).toBe("loginError");
    expect(p.errors).toHaveLength(0);
    p.document.querySelector("form").remove();
    p.dom.reconfigure({ url: "https://self.szu.edu.cn/self/nav_home" });
    expect(p.evaluate("restore", { documentToken: "document-1" }).kind).toBe("portal");
    expect(p.window.__szuNetworkFeeFlow.restored).toBe(true);
    // jsdom deliberately does not navigate. It records each location.replace call.
    expect(p.errors.filter((error) => /navigation/i.test(error.message))).toHaveLength(1);
    expect(p.evaluate("restore", { documentToken: "document-1" }).kind).toBe("stale");
    expect(p.errors.filter((error) => /navigation/i.test(error.message))).toHaveLength(1);
  });
  it("returns a never-ready document to the fixed local route without waiting for its form", () => {
    const p = page({ ready: "loading", html: "" });
    expect(p.evaluate("return").kind).toBe("returning");
    expect(p.window.__szuNetworkFeeFlow.cancelled).toBe(true);
    expect(p.evaluate("return").kind).toBe("stale");
    expect(p.evaluate().kind).toBe("stale");
    expect(p.errors.filter((error) => /navigation/i.test(error.message))).toHaveLength(1);
  });

  it.each(["manual-button", "runtime-handoff", "return-button", "cancel", "newer-generation"])(
    "a queued deadline return cannot interrupt %s",
    (mode) => {
      const p = page();
      p.evaluate();
      const shadow = p.document.querySelector("aside").shadowRoot;
      if (mode === "manual-button") shadow.querySelector("button").click();
      if (mode === "runtime-handoff") p.evaluate("toolbar", { manual: true });
      if (mode === "return-button") {
        shadow.querySelector("a").addEventListener("click", (event) => event.preventDefault());
        shadow.querySelector("a").click();
      }
      if (mode === "cancel") p.evaluate("cancel");
      if (mode === "newer-generation")
        p.evaluate("probe", { generation: "2", newDocumentToken: "document-2" });
      expect(["manual", "stale"]).toContain(p.evaluate("return").kind);
      expect(p.errors).toHaveLength(mode === "return-button" ? 1 : 0);
      if (mode === "return-button") expect(p.errors[0].message).toMatch(/navigation/i);
      p.expectEmpty();
    },
  );

  it("keeps only the new task's toolbar and invalidates the previous context", () => {
    const p = page();
    p.evaluate();
    const previous = p.window.__szuNetworkFeeFlow;
    p.evaluate("probe", { generation: "2", newDocumentToken: "document-2" });
    expect(previous.cancelled).toBe(true);
    expect(previous.toolbar.isConnected).toBe(false);
    expect(p.document.querySelectorAll("#szu-network-fee-toolbar")).toHaveLength(1);
    p.document.querySelector("aside").shadowRoot.querySelector("button").click();
    expect(p.fill({ generation: "2", documentToken: "document-2" }).kind).toBe("manual");
    p.expectEmpty();
  });

  it("checks the native login href even when a page replaces its URL constructor", () => {
    const p = page();
    p.evaluate();
    const FakeURL = p.window.eval(
      '(function () { return {protocol:"https:",hostname:"self.szu.edu.cn",port:"",username:"",password:"",pathname:"/self/nav_login",search:"",hash:""}; })',
    );
    p.window.URL = FakeURL;
    p.dom.reconfigure({ url: "https://evil.test/self/nav_login" });
    expect(p.fill().kind).toBe("invalidForm");
    p.expectEmpty();
  });

  it("does not expose invocation arguments through legacy caller introspection in page getters", () => {
    const p = page({ url: "https://evil.test/" });
    // Deliberately sloppy hostile code. The serialized production function
    // must remain strict, even though native eval is not an ES module.
    p.window.eval(
      'Object.defineProperty(window, "__szuNetworkFeeFlow", { configurable: true, get: function leak() { try { window.stolen = leak.caller.arguments[0].password; } catch {} return undefined; }});',
    );
    expect(p.fill().kind).toBe("stale");
    expect(p.window.stolen).toBeUndefined();
    p.expectEmpty();
  });
});
