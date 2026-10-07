// Native initialization expression, wrapped and invoked by official.rs.
// oxlint-disable-next-line no-unused-expressions
(bootstrap) => {
  if (
    window !== window.top ||
    location.origin !== bootstrap.origin ||
    !location.pathname.startsWith("/xsxkapp/sys/xsxkapp/")
  )
    return;
  const storedGeneration = Number(sessionStorage.getItem("__szuOfficialGeneration") || 0);
  const existingAccount = (() => {
    try {
      return JSON.parse(sessionStorage.getItem("studentInfo") || "null")?.code;
    } catch {
      return undefined;
    }
  })();
  const previous = window.__SZU_OFFICIAL_SESSION__;
  if (previous && previous.generation > bootstrap.generation) return;
  const state = previous || {
    generation: storedGeneration,
    account:
      sessionStorage.getItem("__szuOfficialAccount") ||
      (existingAccount == null ? bootstrap.student : String(existingAccount)),
    changed: 0,
    logout: sessionStorage.getItem("__szuOfficialLogout") === "1",
    expired: false,
  };
  if (state.account !== bootstrap.student) {
    state.read = () => ({
      generation: state.generation,
      account: state.account,
      token: "",
      changed: 0,
      logout: false,
      expired: false,
    });
    window.__SZU_OFFICIAL_SESSION__ = state;
    return;
  }
  const update =
    bootstrap.generation > storedGeneration || !sessionStorage.getItem("__szuOfficialGeneration");
  if (update) {
    state.generation = bootstrap.generation;
    state.account = bootstrap.student;
    state.logout = false;
    state.expired = false;
    sessionStorage.setItem("__szuOfficialGeneration", String(bootstrap.generation));
    sessionStorage.setItem("__szuOfficialAccount", bootstrap.student);
    sessionStorage.setItem("__szuOfficialLogout", "0");
    sessionStorage.setItem("token", bootstrap.token);
    if (bootstrap.initial) {
      for (const key of [
        "studentInfo",
        "currentBatch",
        "currentCampus",
        "dictionary",
        "sysParam",
        "campusList",
      ]) {
        if (bootstrap[key] != null) sessionStorage.setItem(key, JSON.stringify(bootstrap[key]));
        else if (["studentInfo", "currentBatch", "currentCampus"].includes(key))
          sessionStorage.removeItem(key);
      }
    }
  }
  state.read = () => ({
    generation: state.generation,
    account: state.account,
    token: sessionStorage.getItem("token") || "",
    changed: state.changed,
    logout: state.logout || sessionStorage.getItem("__szuOfficialLogout") === "1",
    expired: state.expired,
  });
  state.invalidate = (generation) => {
    state.generation = generation;
    state.logout = true;
    sessionStorage.setItem("__szuOfficialGeneration", String(generation));
    sessionStorage.setItem("__szuOfficialLogout", "1");
    sessionStorage.removeItem("token");
  };
  window.__SZU_OFFICIAL_SESSION__ = state;
  if (!previous) {
    const pathFor = (url) => {
      try {
        const u = new URL(url, location.href);
        return u.origin === bootstrap.origin ? u.pathname : "";
      } catch {
        return "";
      }
    };
    const logout = (path) => /\/student\/(?:auth\/)?logout\.do$/.test(path);
    const observe = (path, payload) => {
      if (path.endsWith("/student/check/login.do") && String(payload?.code) === "1") {
        const account = payload?.data?.number ?? payload?.data?.code;
        if (account != null) {
          state.account = String(account);
          sessionStorage.setItem("__szuOfficialAccount", state.account);
        }
        state.logout = false;
        state.expired = false;
        sessionStorage.setItem("__szuOfficialLogout", "0");
      }
      if (["302", "401"].includes(String(payload?.code))) state.expired = true;
      if (/\/elective\/(?:deleteVolunteer|volunteer)\.do$/.test(path)) state.changed++;
    };
    const requests = new WeakMap();
    const open = XMLHttpRequest.prototype.open,
      send = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.open = function (method, url, ...args) {
      requests.set(this, pathFor(url));
      return open.call(this, method, url, ...args);
    };
    XMLHttpRequest.prototype.send = function (body) {
      const path = requests.get(this) || "";
      if (logout(path)) {
        state.logout = true;
        sessionStorage.setItem("__szuOfficialLogout", "1");
      }
      if (path)
        this.addEventListener(
          "load",
          () => {
            try {
              observe(
                path,
                this.responseType === "json" ? this.response : JSON.parse(this.responseText),
              );
            } catch {}
          },
          { once: true },
        );
      return send.call(this, body);
    };
    const fetch = window.fetch;
    window.fetch = function (input, options) {
      const path = pathFor(typeof input === "string" ? input : input?.url);
      if (logout(path)) {
        state.logout = true;
        sessionStorage.setItem("__szuOfficialLogout", "1");
      }
      const response = fetch.call(this, input, options);
      if (path)
        response
          .then((r) => r.clone().json())
          .then((p) => observe(path, p))
          .catch(() => {});
      return response;
    };
  }
  // Run the school's own read-only student initialization for restricted accounts.
  // No fabricated studentInfo and no password submission are involved.
  const initialize = () => {
    if (
      !state.logout &&
      location.pathname.endsWith("/index.do") &&
      !bootstrap.studentInfo &&
      window.CVStudentLogin?.studentInfo
    )
      window.CVStudentLogin.studentInfo(bootstrap.student);
  };
  if (document.readyState === "loading")
    document.addEventListener("DOMContentLoaded", initialize, { once: true });
  else initialize();
};
