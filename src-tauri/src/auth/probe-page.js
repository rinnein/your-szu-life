export default function authProbePage({ operation, generation, budgetMs }) {
  if (
    window !== window.top ||
    location.origin !== "https://authserver.szu.edu.cn" ||
    !location.pathname.startsWith("/personalInfo/")
  )
    return null;
  const previous = window.__SZU_AUTH_PROBE_STATE__;
  if (operation === "poll") {
    if (previous?.generation !== generation) return null;
    const result = previous.result;
    previous.result = null;
    return result;
  }
  if (previous?.generation === generation) return null;
  if (previous && BigInt(previous.generation) > BigInt(generation)) return null;
  previous?.dispose();
  const deadline = Date.now() + budgetMs;
  let timer;
  let retry = 0;
  let controller;
  let requestTimer;
  const state = { generation, result: null, disposed: false, pending: false, dispose };
  window.__SZU_AUTH_PROBE_STATE__ = state;
  function dispose() {
    state.disposed = true;
    clearTimeout(timer);
    clearTimeout(requestTimer);
    controller?.abort();
    window.removeEventListener("pagehide", dispose);
  }
  const publish = (result) => {
    if (!state.disposed) state.result = result;
  };
  async function request() {
    if (state.disposed || state.pending) return;
    const remaining = deadline - Date.now();
    if (remaining <= 0) {
      publish({ ok: false, reason: "timeout" });
      return;
    }
    state.pending = true;
    controller = new AbortController();
    requestTimer = setTimeout(() => controller.abort(), Math.min(30000, remaining));
    let transient = false;
    try {
      const response = await fetch("/personalInfo/common/getUserConf", {
        credentials: "include",
        signal: controller.signal,
      });
      const text = await response.text();
      transient = response.status === 408 || response.status === 429 || response.status >= 500;
      if (!transient)
        publish({ ok: response.ok, status: response.status, url: response.url, text });
    } catch {
      transient = true;
    } finally {
      clearTimeout(requestTimer);
      state.pending = false;
    }
    if (transient && !state.disposed) {
      const delay = Math.min(1000 * 2 ** Math.min(retry++, 3), Math.max(0, deadline - Date.now()));
      timer = setTimeout(request, delay);
    }
  }
  window.addEventListener("pagehide", dispose, { once: true });
  void request();
  return null;
}
