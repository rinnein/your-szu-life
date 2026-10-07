import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import fs from "node:fs";
const script = fs.readFileSync(new URL("./academic-page.js", import.meta.url), "utf8");
function setup(host = "ehall.szu.edu.cn") {
  class XHR {
    open() {}
    send(body) {
      this.sent = body;
    }
    addEventListener(name, fn) {
      this.listener = fn;
    }
  }
  const window = {
    fetch: () => Promise.resolve({ clone: () => ({ json: () => Promise.resolve({}) }) }),
  };
  window.top = window;
  const context = {
    window,
    location: {
      hostname: host,
      origin: `https://${host}`,
      href: `https://${host}/jwapp/sys/xywccx/*default/index.do`,
    },
    XMLHttpRequest: XHR,
    URL,
    URLSearchParams,
    WeakMap,
  };
  vm.createContext(context);
  vm.runInContext(script, context);
  return { window, XHR };
}
test("captures only academic context fields from the official request", () => {
  const { window, XHR } = setup();
  const x = new XHR();
  x.open("POST", "/jwapp/sys/xywccx/modules/xywccx/cxscfa.do");
  x.send("XH=test&PYFADM=plan&BYNJDM=year&SCLBDM=04&token=must-not-capture");
  assert.deepEqual(JSON.parse(JSON.stringify(window.__SZU_ACADEMIC_CONTEXT__)), {
    XH: "test",
    PYFADM: "plan",
    BYNJDM: "year",
    SCLBDM: "04",
  });
  assert.equal(x.sent.includes("token="), true);
});
test("does not capture unrelated requests or a lookalike domain", () => {
  for (const [host, path] of [
    ["ehall.szu.edu.cn", "/other"],
    ["ehall.szu.edu.cn.evil.test", "/jwapp/sys/xywccx/modules/xywccx/cxscfa.do"],
  ]) {
    const { window, XHR } = setup(host);
    const x = new XHR();
    x.open("POST", path);
    x.send("XH=test&PYFADM=plan&BYNJDM=year&SCLBDM=04");
    assert.equal(window.__SZU_ACADEMIC_CONTEXT__, undefined);
  }
});
test("captures response context without modifying the response", () => {
  const { window, XHR } = setup();
  const x = new XHR();
  x.open("POST", "/jwapp/sys/xywccx/modules/xywccx/cxscfa.do");
  x.send("");
  x.responseType = "json";
  x.response = {
    datas: {
      cxscfa: {
        rows: [{ XH: "test", PYFADM: "plan", BYNJDM: "year", SCLBDM: "04", cookie: "secret" }],
      },
    },
  };
  x.listener();
  assert.equal(window.__SZU_ACADEMIC_CONTEXT__.PYFADM, "plan");
  assert.equal(window.__SZU_ACADEMIC_CONTEXT__.cookie, undefined);
  assert.equal(x.response.datas.cxscfa.rows[0].cookie, "secret");
});
