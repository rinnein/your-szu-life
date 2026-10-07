import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import fs from "node:fs";
const source = fs
  .readFileSync(new URL("./official-page.js", import.meta.url), "utf8")
  .trim()
  .replace(/;$/, "");
function setup(values = new Map(), host = "bkxk.szu.edu.cn") {
  class XHR {
    open(method, url) {
      this.url = url;
    }
    send(body) {
      this.body = body;
      this.sends = (this.sends || 0) + 1;
    }
    addEventListener(_, fn) {
      this.receive = fn;
    }
  }
  const storage = {
    getItem: (k) => values.get(k) ?? null,
    setItem: (k, v) => values.set(k, String(v)),
    removeItem: (k) => values.delete(k),
  };
  const calls = [];
  const fetch = (...args) => {
    calls.push(args);
    return Promise.resolve({ clone: () => ({ json: () => Promise.resolve({ code: "1" }) }) });
  };
  const window = {
    fetch,
    CVStudentLogin: { studentInfo: (id) => calls.push(["studentInfo", id]) },
  };
  window.top = window;
  const ctx = {
    window,
    XMLHttpRequest: XHR,
    sessionStorage: storage,
    location: {
      origin: `http://${host}`,
      href: `http://${host}/xsxkapp/sys/xsxkapp/*default/index.do`,
      pathname: "/xsxkapp/sys/xsxkapp/*default/index.do",
    },
    document: { readyState: "complete" },
    URL,
    WeakMap,
  };
  vm.createContext(ctx);
  return {
    ctx,
    window,
    XHR,
    storage,
    values,
    calls,
    inject: (patch = {}) => {
      ctx.bootstrap = {
        origin: "http://bkxk.szu.edu.cn",
        student: "demo",
        token: "synthetic-token",
        generation: 1,
        initial: true,
        ...patch,
      };
      vm.runInContext(`(${source})(bootstrap)`, ctx);
    },
  };
}
test("injects credentials only at the exact school origin and never fabricates restricted studentInfo", () => {
  const s = setup();
  s.inject();
  assert.equal(s.storage.getItem("token"), "synthetic-token");
  assert.equal(s.storage.getItem("studentInfo"), null);
  assert.deepEqual(s.calls, [["studentInfo", "demo"]]);
  const wrong = setup(new Map(), "bkxk.szu.edu.cn.evil.test");
  wrong.inject();
  assert.equal(wrong.storage.getItem("token"), null);
});
test("requests remain direct, and explicit logout stays revoked across page navigation", () => {
  const s = setup();
  s.inject({ studentInfo: { code: "demo" } });
  const xhr = new s.XHR();
  xhr.open("POST", "/xsxkapp/sys/xsxkapp/student/auth/logout.do");
  xhr.send("original-body");
  assert.equal(xhr.body, "original-body");
  assert.equal(xhr.sends, 1);
  assert.equal(s.window.__SZU_OFFICIAL_SESSION__.read().logout, true);
  s.window.__SZU_OFFICIAL_SESSION__.invalidate(2);
  const next = setup(s.values);
  next.inject();
  assert.equal(next.storage.getItem("token"), null);
  assert.equal(next.window.__SZU_OFFICIAL_SESSION__.read().logout, true);
  next.inject({ generation: 3, token: "new-session", initial: false });
  assert.equal(next.storage.getItem("token"), "new-session");
  assert.equal(next.window.__SZU_OFFICIAL_SESSION__.read().logout, false);
});
test("old window bootstrap cannot overwrite a newer session after navigation", () => {
  const s = setup();
  s.inject();
  s.inject({ generation: 4, token: "new-session", initial: false });
  s.inject({ generation: 2, token: "old-session" });
  assert.equal(s.storage.getItem("token"), "new-session");
  const next = setup(s.values);
  next.inject();
  assert.equal(next.storage.getItem("token"), "new-session");
  assert.equal(next.window.__SZU_OFFICIAL_SESSION__.read().generation, 4);
});
test("a different official login account is reported and not overwritten by the app", () => {
  const s = setup();
  s.inject();
  const xhr = new s.XHR();
  xhr.open("POST", "/xsxkapp/sys/xsxkapp/student/check/login.do");
  xhr.send("untouched");
  xhr.responseType = "json";
  xhr.response = { code: "1", data: { number: "other-account", token: "other-token" } };
  s.storage.setItem("token", "other-token");
  xhr.receive();
  assert.equal(s.window.__SZU_OFFICIAL_SESSION__.read().account, "other-account");
  const next = setup(s.values);
  next.inject();
  assert.equal(next.storage.getItem("token"), "other-token");
  assert.equal(next.window.__SZU_OFFICIAL_SESSION__.read().account, "other-account");
});
test("opening or observing an official selection does not proxy or suppress its write", () => {
  const s = setup();
  s.inject();
  const xhr = new s.XHR();
  xhr.open("POST", "/xsxkapp/sys/xsxkapp/elective/volunteer.do");
  xhr.send("addParam=original");
  xhr.responseType = "json";
  xhr.response = { code: "1" };
  xhr.receive();
  assert.equal(xhr.sends, 1);
  assert.equal(xhr.body, "addParam=original");
  assert.equal(s.window.__SZU_OFFICIAL_SESSION__.read().changed, 1);
});
