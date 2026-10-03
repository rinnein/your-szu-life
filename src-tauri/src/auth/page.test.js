/** @vitest-environment node */
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { JSDOM } from "jsdom";
import authLoginPage from "./page.js";

const LOGIN = "https://authserver.szu.edu.cn/authserver/login";
const CREDENTIALS = { identifier: "1234567890", password: "offline-test-only" };
const FORM =
  '<form id="pwdFromId"><input id="username"><input id="password" type="password"><input id="rememberMe" type="checkbox"><div id="captchaDiv" style="display:none"><input id="captcha"></div><span id="showErrorTip" class="item-error-tip"></span><button id="login_submit" type="button">登录</button></form>';
const pages = [];
const flush = async () => {
  for (let i = 0; i < 15; i++) await Promise.resolve();
};

function native(automatic = true) {
  let phase = automatic ? "loading" : "manual";
  let deadline = Date.now() + 120000;
  return {
    control(operation) {
      if (Date.now() >= deadline && phase !== "manual") phase = "manual";
      if (operation === "formReady" && phase === "loading") {
        phase = "ready";
        deadline = Date.now() + 30000;
      }
      if (operation === "claim") {
        if (phase !== "ready") throw new Error("already consumed");
        phase = "submitted";
        deadline = Date.now() + 90000;
      }
      if (operation === "stop") phase = "manual";
      return { phase, remainingMs: Math.max(0, deadline - Date.now()) };
    },
  };
}

function page({
  url = LOGIN,
  html = `<div hidden>${FORM}</div><main class="login-main"><div class="m-account">${FORM}</div></main>`,
  backend = native(),
  automatic = true,
  prefill = CREDENTIALS,
  deferCapture = false,
  ready = "complete",
} = {}) {
  const dom = new JSDOM(html, { url, runScripts: "outside-only" });
  pages.push(dom);
  const { window } = dom;
  const { document } = window;
  Object.defineProperty(document, "readyState", { configurable: true, value: ready });
  window.Element.prototype.getClientRects = function () {
    return this.isConnected ? [{}] : [];
  };
  window.setTimeout = setTimeout;
  window.clearTimeout = clearTimeout;
  window.setInterval = setInterval;
  window.clearInterval = clearInterval;
  const submit = vi.fn();
  window.HTMLFormElement.prototype.submit = submit;
  let finishCapture;
  const captured = deferCapture
    ? new Promise((resolve) => {
        finishCapture = resolve;
      })
    : Promise.resolve();
  const invoke = vi.fn(async (command, args) => {
    if (command === "control_auth_recovery") return backend.control(args.operation);
    if (command === "capture_auth_credentials") return captured;
    throw new Error("unexpected command");
  });
  window.__TAURI_INTERNALS__ = { invoke };
  window.needCaptcha = false;
  const official = vi.fn();
  window.startLogin = official;
  const ajaxHandlers = new Map();
  window.jQuery = () => ({
    on: (name, handler) => ajaxHandlers.set(name, handler),
    off: () => ajaxHandlers.clear(),
  });
  window.checkNeedCaptcha = vi.fn();
  const captcha = (isNeed = false, status = 200) => {
    window.needCaptcha = isNeed;
    for (const handler of ajaxHandlers.values())
      handler(
        {},
        { status, responseJSON: { isNeed } },
        { url: "/authserver/checkNeedCaptcha.htl" },
      );
  };
  const evaluate = (overrides) =>
    window.eval(
      `(${authLoginPage.toString()})(${JSON.stringify({ generation: "1", documentId: "1", automatic, allowQrLogin: true, prefill, ...overrides })})`,
    );
  const field = (id) => document.querySelector(`.login-main #${id}`);
  return { window, document, official, invoke, evaluate, captcha, field, finishCapture, submit };
}

afterEach(() => {
  for (const dom of pages.splice(0)) {
    dom.window.__SZU_AUTH_LOGIN__?.dispose();
    dom.window.close();
  }
  vi.useRealTimers();
});

describe("official auth automatic recovery", () => {
  it.each([20, 90, 119])("keeps a %s-second render within its own budget", async (seconds) => {
    vi.useFakeTimers();
    const p = page({ html: '<main class="login-main"></main>' });
    p.evaluate();
    await flush();
    await vi.advanceTimersByTimeAsync(seconds * 1000);
    p.document.querySelector("main").innerHTML = FORM;
    await flush();
    await vi.advanceTimersByTimeAsync(25000);
    p.captcha();
    await flush();
    expect(p.official).toHaveBeenCalledTimes(1);
    expect(p.document.body.textContent).not.toContain("超时");
  });

  it("does not restart automation when rendering completes after two minutes", async () => {
    vi.useFakeTimers();
    const p = page({ html: '<main class="login-main"></main>' });
    p.evaluate();
    await flush();
    await vi.advanceTimersByTimeAsync(120000);
    p.document.querySelector("main").innerHTML = FORM;
    await flush();
    p.captcha();
    await flush();
    expect(p.official).not.toHaveBeenCalled();
    expect(p.field("password").value).toBe("");
  });

  it("confirms a password submission only after official validation reaches the form", async () => {
    const p = page();
    p.official.mockImplementation(() => p.field("pwdFromId").submit());
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    expect(p.invoke).toHaveBeenCalledWith(
      "control_auth_recovery",
      expect.objectContaining({ operation: "passwordSubmitted" }),
    );
    expect(p.submit).toHaveBeenCalledTimes(1);
  });

  it("does not confirm a rejected password, and clears it before a QR switch", async () => {
    vi.useFakeTimers();
    const p = page();
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    expect(p.submit).not.toHaveBeenCalled();
    expect(p.invoke.mock.calls.some(([, args]) => args.operation === "passwordSubmitted")).toBe(
      false,
    );
    const tab = p.document.createElement("button");
    tab.id = "qrLoginSpan";
    p.document.body.append(tab);
    const switched = vi.fn(() => {
      expect(p.invoke).toHaveBeenCalledWith(
        "control_auth_recovery",
        expect.objectContaining({ operation: "discardPassword" }),
      );
    });
    tab.addEventListener("click", switched);
    tab.click();
    await flush();
    expect(switched).toHaveBeenCalledTimes(1);
  });
  it.each(["m-account", "pwd_login"])(
    "uses only the active %s form and captures before official submission",
    async (layout) => {
      const p = page({
        html: `<div hidden>${FORM}</div><main class="login-main"><div class="${layout}">${FORM}</div></main>`,
        deferCapture: true,
      });
      p.evaluate();
      await flush();
      expect(p.field("username").value).toBe(CREDENTIALS.identifier);
      expect(p.field("rememberMe").checked).toBe(true);
      expect(p.document.querySelector("div[hidden] #password").value).toBe("");
      expect(p.window.checkNeedCaptcha).toHaveBeenCalledTimes(1);
      expect(p.official).not.toHaveBeenCalled();
      p.captcha();
      await flush();
      expect(p.invoke).toHaveBeenCalledWith(
        "capture_auth_credentials",
        expect.objectContaining({ capture: CREDENTIALS }),
      );
      expect(p.official).not.toHaveBeenCalled();
      p.finishCapture();
      await flush();
      expect(p.official).toHaveBeenCalledExactlyOnceWith(p.field("login_submit"));
      expect(p.window.localStorage.length).toBe(0);
      expect(p.window.sessionStorage.length).toBe(0);
    },
  );

  it("submits once across reinjection, mutation, reload and error redirects", async () => {
    const backend = native();
    const p = page({ backend });
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    p.evaluate();
    p.document.body.append(p.document.createElement("div"));
    await flush();
    expect(p.official).toHaveBeenCalledTimes(1);
    const redirected = page({ backend });
    redirected.evaluate({ documentId: "2" });
    await flush();
    redirected.captcha();
    await flush();
    expect(redirected.field("password").value).toBe("");
    expect(redirected.official).not.toHaveBeenCalled();
    expect(redirected.document.body.textContent).toContain("已尝试登录");
  });

  it("waits for asynchronous form and script initialization", async () => {
    vi.useFakeTimers();
    const p = page({ html: '<main class="login-main"></main>', ready: "loading" });
    p.evaluate();
    await flush();
    p.document.querySelector("main").innerHTML = FORM;
    await flush();
    expect(p.field("password").value).toBe("");
    Object.defineProperty(p.document, "readyState", { value: "complete" });
    await vi.advanceTimersByTimeAsync(500);
    p.captcha();
    await flush();
    expect(p.official).toHaveBeenCalledTimes(1);
  });

  it.each(["captcha", "error", "network"])(
    "hands %s to the user without submitting",
    async (reason) => {
      const p = page();
      p.evaluate();
      await flush();
      if (reason === "captcha") p.captcha(true);
      else if (reason === "network") p.captcha(false, 503);
      else {
        p.field("showErrorTip").textContent = "密码错误";
        p.captcha();
      }
      await flush();
      expect(p.official).not.toHaveBeenCalled();
      expect(p.document.body.textContent).toMatch(/手动|异常/);
      p.window.needCaptcha = false;
      p.field("showErrorTip").textContent = "";
      p.field("password").value = "corrected-test-password";
      p.field("login_submit").click();
      await flush();
      expect(p.official).toHaveBeenCalledTimes(1);
    },
  );

  it("preserves user edits made while credential capture is pending", async () => {
    const p = page({ deferCapture: true });
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    p.field("password").value = "user-edit";
    p.field("password").dispatchEvent(new p.window.Event("input", { bubbles: true }));
    p.finishCapture();
    await flush();
    expect(p.official).not.toHaveBeenCalled();
    expect(p.field("password").value).toBe("user-edit");
    p.field("login_submit").click();
    await flush();
    expect(p.official).toHaveBeenCalledTimes(1);
  });

  it("stops when the user switches login mode before captcha completes", async () => {
    const p = page();
    p.evaluate();
    await flush();
    const tab = p.document.createElement("button");
    tab.id = "qrLoginSpan";
    p.document.body.append(tab);
    tab.click();
    p.captcha();
    await flush();
    expect(p.official).not.toHaveBeenCalled();
  });

  it.each([false, true])("hands off after the %s submission deadline", async (submitted) => {
    vi.useFakeTimers();
    const p = page();
    p.evaluate();
    await flush();
    if (submitted) {
      p.captcha();
      await flush();
    }
    await vi.advanceTimersByTimeAsync(submitted ? 90000 : 30000);
    expect(p.document.body.textContent).toContain("超时");
    expect(p.official).toHaveBeenCalledTimes(submitted ? 1 : 0);
    await vi.advanceTimersByTimeAsync(30000);
    expect(p.official).toHaveBeenCalledTimes(submitted ? 1 : 0);
  });

  it("keeps first login manual and preserves keyboard submission", async () => {
    const p = page({ backend: native(false), automatic: false, prefill: null });
    p.evaluate();
    await flush();
    p.field("username").value = CREDENTIALS.identifier;
    p.field("password").value = CREDENTIALS.password;
    p.field("password").dispatchEvent(
      new p.window.KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
    );
    await flush();
    expect(p.official).toHaveBeenCalledTimes(1);
    expect(p.invoke.mock.calls.some(([, args]) => args.operation === "claim")).toBe(false);
  });

  it.each([
    "http://authserver.szu.edu.cn/authserver/login",
    "https://authserver.szu.edu.cn.evil.test/authserver/login",
    "https://authserver.szu.edu.cn:8443/authserver/login",
    "https://authserver.szu.edu.cn/authserver/login-extra",
  ])("ignores untrusted document %s", async (url) => {
    const p = page({ url });
    p.evaluate();
    await flush();
    expect(p.invoke).not.toHaveBeenCalled();
    expect(p.field("password").value).toBe("");
  });

  it("does not fill or submit when the native generation was cancelled", async () => {
    const p = page({
      backend: {
        control() {
          throw new Error("stale");
        },
      },
    });
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    expect(p.field("password").value).toBe("");
    expect(p.official).not.toHaveBeenCalled();
  });

  it("revokes delayed submission on page departure and replaces old hooks", async () => {
    const p = page({ deferCapture: true });
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    p.window.dispatchEvent(new p.window.Event("pagehide"));
    p.finishCapture();
    await flush();
    expect(p.official).not.toHaveBeenCalled();
    expect(p.window.startLogin).toBe(p.official);
  });

  it("ignores an old queued injection without disposing the current document", async () => {
    const p = page();
    p.evaluate({ documentId: "2" });
    await flush();
    const current = p.window.__SZU_AUTH_LOGIN__;
    p.evaluate({ documentId: "1" });
    await flush();
    expect(p.window.__SZU_AUTH_LOGIN__).toBe(current);
    p.captcha();
    await flush();
    expect(p.official).toHaveBeenCalledTimes(1);
  });

  it("does not submit when a claimed permit arrives after manual takeover", async () => {
    const backend = native();
    let release;
    const p = page({
      backend: {
        control(operation) {
          const result = backend.control(operation);
          return operation === "claim"
            ? new Promise((resolve) => {
                release = () => resolve(result);
              })
            : result;
        },
      },
    });
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    expect(release).toBeTypeOf("function");
    p.window.__SZU_AUTH_LOGIN__.stop();
    release();
    await flush();
    expect(p.official).not.toHaveBeenCalled();
  });

  it("does not bypass a rejected credential capture", async () => {
    const p = page();
    const implementation = p.invoke.getMockImplementation();
    p.invoke.mockImplementation((command, args) =>
      command === "capture_auth_credentials"
        ? Promise.reject(new Error("capture rejected"))
        : implementation(command, args),
    );
    p.evaluate();
    await flush();
    p.captcha();
    await flush();
    expect(p.official).not.toHaveBeenCalled();
    expect(p.document.body.textContent).toContain("手动");
  });

  it("blocks initial QR login while keeping the password form usable", async () => {
    const p = page({ backend: native(false), automatic: false, prefill: null });
    p.document.body.insertAdjacentHTML(
      "beforeend",
      '<div class="qrcode-middle"><a id="qrLogin_a">扫码</a></div>',
    );
    const qr = vi.fn();
    p.document.querySelector("#qrLogin_a").addEventListener("click", qr);
    p.evaluate({ allowQrLogin: false });
    await flush();
    p.document.querySelector("#qrLogin_a").click();
    expect(qr).not.toHaveBeenCalled();
    expect(p.document.body.textContent).toContain("二维码登录已禁用");
  });
});
