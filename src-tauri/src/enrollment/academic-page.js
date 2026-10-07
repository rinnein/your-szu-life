(() => {
  if (location.hostname !== "ehall.szu.edu.cn" || window !== window.top) return;
  const allowed = (url) => {
    try {
      const u = new URL(url, location.href);
      return (
        u.origin === location.origin && u.pathname === "/jwapp/sys/xywccx/modules/xywccx/cxscfa.do"
      );
    } catch {
      return false;
    }
  };
  const capture = (row) => {
    if (!row || typeof row !== "object") return;
    const result = {};
    for (const key of ["XH", "PYFADM", "BYNJDM", "SCLBDM", "CZSJ"]) {
      const value = row[key];
      if (typeof value === "string" || typeof value === "number") result[key] = String(value);
    }
    if (["XH", "PYFADM", "BYNJDM", "SCLBDM"].every((key) => result[key]))
      window.__SZU_ACADEMIC_CONTEXT__ = result;
  };
  const receive = (payload) => capture(payload?.datas?.cxscfa?.rows?.[0]);
  const requests = new WeakMap();
  const open = XMLHttpRequest.prototype.open;
  const send = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (method, url, ...args) {
    requests.set(this, allowed(url));
    return open.call(this, method, url, ...args);
  };
  XMLHttpRequest.prototype.send = function (body) {
    if (requests.get(this)) {
      if (typeof body === "string") capture(Object.fromEntries(new URLSearchParams(body)));
      this.addEventListener(
        "load",
        () => {
          try {
            receive(this.responseType === "json" ? this.response : JSON.parse(this.responseText));
          } catch {}
        },
        { once: true },
      );
    }
    return send.call(this, body);
  };
  const fetch = window.fetch;
  window.fetch = function (input, options) {
    const result = fetch.call(this, input, options);
    if (allowed(typeof input === "string" ? input : input?.url)) {
      if (typeof options?.body === "string")
        capture(Object.fromEntries(new URLSearchParams(options.body)));
      result
        .then((response) => response.clone().json())
        .then(receive)
        .catch(() => {});
    }
    return result;
  };
})();
