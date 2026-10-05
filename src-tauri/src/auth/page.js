export default function authLoginPage(options) {
  if (
    location.protocol !== "https:" ||
    location.host !== "authserver.szu.edu.cn" ||
    location.pathname !== "/authserver/login" ||
    window !== window.top
  )
    return;
  if (!document.documentElement) return;

  const { generation, documentId, allowQrLogin: qrAllowed } = options;
  let savedCredentials = options.prefill;
  options.prefill = null;
  const previous = window.__SZU_AUTH_LOGIN__;
  if (
    previous &&
    (BigInt(previous.generation) > BigInt(generation) ||
      (previous.generation === generation && BigInt(previous.documentId) > BigInt(documentId)))
  )
    return;
  if (previous?.generation === generation && previous.documentId === documentId) return;
  previous?.dispose();
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (typeof invoke !== "function") return;
  const control = (operation) =>
    invoke("control_auth_recovery", { generation, documentId, operation });
  let disposed = false;
  let manual = false;
  let automatic = false;
  let checking = false;
  let captchaChecked = false;
  let initialized = false;
  let formReady = false;
  let credentialReset = Promise.resolve(true);
  let forwardingForm = false;
  const originalFormSubmit = HTMLFormElement.prototype.submit;
  let wrappedFormSubmit;
  let replayingLoginMode = false;
  let ticking = false;
  let observer;
  let interval;
  let deadline;
  let stopVersion = 0;
  let applyingCredentials = false;
  let wrappedStartLogin;
  let originalStartLogin;
  const removers = [];
  const listen = (type, handler) => {
    document.addEventListener(type, handler, true);
    removers.push(() => document.removeEventListener(type, handler, true));
  };
  const notice = (message) => {
    if (disposed || !document.body) return;
    let element = document.getElementById("szu-auth-recovery-status");
    if (!element) {
      element = document.createElement("p");
      element.id = "szu-auth-recovery-status";
      element.setAttribute("role", "status");
      (document.querySelector(".login-main") || document.body).prepend(element);
    }
    if (element.textContent !== message) element.textContent = message;
  };
  const stop = (message = "自动登录已停止，请在官方页面手动完成登录。") => {
    manual = true;
    stopVersion += 1;
    savedCredentials = null;
    clearTimeout(deadline);
    void control("stop").catch(() => undefined);
    notice(message);
  };
  const dispose = () => {
    disposed = true;
    stopVersion += 1;
    savedCredentials = null;
    observer?.disconnect();
    clearInterval(interval);
    clearTimeout(deadline);
    removers.forEach((remove) => remove());
    window.jQuery?.(document).off(".szuAuthRecovery");
    if (window.startLogin === wrappedStartLogin) window.startLogin = originalStartLogin;
    candidateCapture = null;
    if (HTMLFormElement.prototype.submit === wrappedFormSubmit)
      HTMLFormElement.prototype.submit = originalFormSubmit;
  };
  window.__SZU_AUTH_LOGIN__ = { generation, documentId, stop, dispose };
  const autofilledForms = new WeakSet();
  let submitting = false;
  let allowOfficialSubmit = false;
  let candidateCapture = null;

  // The official page retains hidden templates with duplicated IDs. Only the
  // form copied into .login-main is live, so never fall back to global fields.
  // Desktop uses .m-account while mobile uses .pwd_login; querying from the
  // active form supports both layouts without crossing into hidden templates.
  const loginForm = () => document.querySelector(".login-main #pwdFromId");
  const formField = (selector) => loginForm()?.querySelector(selector);
  const accountInput = () => formField("#username");
  const passwordInput = () => formField("#password");
  const loginButton = () => formField("#login_submit");
  const rememberInput = () => formField("#rememberMe");
  const isActiveLoginForm = (target) => {
    if (!(target instanceof HTMLFormElement)) return false;
    const form = loginForm();
    return !!form && target === form;
  };
  const isActiveFormTarget = (target) => {
    const form = loginForm();
    return !!form && target instanceof Node && form.contains(target);
  };
  const isActiveLoginButton = (target) => {
    if (!(target instanceof Element)) return false;
    const button = target.closest(".login-main #pwdFromId #login_submit");
    return !!button && button === loginButton();
  };
  const setCaptureError = (message) => {
    const error = document.querySelector(
      ".login-main #showErrorTip, .login-main #pwdErrorTip, .login-main #formErrorTip, .login-main .m-account .error-tip, .login-main .pwd_login .error-tip",
    );
    if (error) {
      error.textContent = message;
      error.classList.remove("hide");
      error.style.display = "block";
    }
  };
  const applySavedCredentials = () => {
    if (!savedCredentials || manual || disposed) return;
    const form = loginForm();
    const account = accountInput();
    const password = passwordInput();
    if (!form || !account || !password || autofilledForms.has(form)) return;
    // The values are only injected into the active account/password form on
    // the HTTPS official login origin. Mark this specific form so the user can
    // still deliberately edit or clear either field afterwards.
    account.value = savedCredentials.identifier;
    password.value = savedCredentials.password;
    applyingCredentials = true;
    for (const input of [account, password]) {
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    }
    applyingCredentials = false;
    autofilledForms.add(form);
  };
  const disableQrLogin = () => {
    if (qrAllowed) return;
    for (const area of document.querySelectorAll(".qrcode-middle")) {
      area.setAttribute("aria-disabled", "true");
      if (getComputedStyle(area).position === "static") area.style.position = "relative";
      if (area.querySelector("[data-szu-app-qr-disabled]")) continue;
      const overlay = document.createElement("div");
      overlay.dataset.szuAppQrDisabled = "true";
      overlay.setAttribute("role", "note");
      overlay.textContent = "二维码登录已禁用\n请使用学号/校园卡号和密码登录，以便完成账户设置";
      Object.assign(overlay.style, {
        position: "absolute",
        inset: "0",
        zIndex: "2147483647",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "20px",
        boxSizing: "border-box",
        textAlign: "center",
        whiteSpace: "pre-line",
        color: "#6b2b2b",
        background: "rgba(255,248,248,.97)",
        border: "1px solid #e6b6b6",
        borderRadius: "6px",
        fontSize: "14px",
        lineHeight: "1.6",
        fontWeight: "600",
        cursor: "not-allowed",
        pointerEvents: "auto",
      });
      area.append(overlay);
    }
  };
  const prepare = () => {
    disableQrLogin();
    const form = loginForm();
    const account = accountInput();
    const password = passwordInput();
    const button = loginButton();
    const remember = rememberInput();
    if (!form || !account || !password || !button) return null;
    if (remember && !remember.checked) {
      applyingCredentials = true;
      remember.checked = true;
      remember.dispatchEvent(new Event("input", { bubbles: true }));
      remember.dispatchEvent(new Event("change", { bubbles: true }));
      applyingCredentials = false;
    }
    if (!account.value.trim() || !password.value) return null;
    return { identifier: account.value.trim(), password: password.value };
  };
  const discardPassword = () => {
    candidateCapture = null;
    credentialReset = control("discardPassword").then(
      () => true,
      () => false,
    );
  };
  const captureCandidate = async () => {
    if (!(await credentialReset)) return false;
    const capture = prepare();
    if (!capture) return false;

    const key = `${capture.identifier}\u0000${capture.password}`;
    if (candidateCapture?.key === key) return candidateCapture.promise;
    const promise = invoke("capture_auth_credentials", { capture, generation, documentId })
      .then(() => true)
      .catch(() => false);
    candidateCapture = { key, promise };
    const captured = await promise;
    if (!captured && candidateCapture?.key === key) candidateCapture = null;
    return captured;
  };
  const submitPasswordForm = async (form) => {
    if (forwardingForm || disposed) return;
    forwardingForm = true;
    const version = stopVersion;
    try {
      await control("passwordSubmitted");
      if (!disposed && version === stopVersion && form === loginForm() && form.isConnected) {
        originalFormSubmit.call(form);
      }
    } catch {
      stop("未能保存登录提交状态，请手动重试。");
    } finally {
      forwardingForm = false;
    }
  };
  wrappedFormSubmit = function (...args) {
    if (!isActiveLoginForm(this)) return originalFormSubmit.apply(this, args);
    if (allowOfficialSubmit) void submitPasswordForm(this);
    else void bridgeSubmit();
  };
  HTMLFormElement.prototype.submit = wrappedFormSubmit;
  const continueOfficialLogin = (button) => {
    if (!button || button !== loginButton() || typeof window.startLogin !== "function") {
      throw new Error("active login button or startLogin unavailable");
    }
    allowOfficialSubmit = true;
    window.startLogin(button);
  };
  const bridgeSubmit = async (auto = false) => {
    if (submitting) return;
    const capture = prepare();
    const button = loginButton();
    if (!capture || !button) {
      setCaptureError("未能读取当前账号密码表单，请刷新后重新输入");
      return;
    }
    const form = loginForm();
    const version = stopVersion;
    let continued = false;
    submitting = true;
    button.setAttribute("aria-disabled", "true");
    try {
      if (!(await captureCandidate())) throw new Error("credential capture failed");
      if (
        disposed ||
        loginForm() !== form ||
        accountInput()?.value.trim() !== capture.identifier ||
        passwordInput()?.value !== capture.password ||
        (auto && (manual || version !== stopVersion))
      )
        return;
      if (auto) {
        const permission = await control("claim");
        if (
          permission.phase !== "submitted" ||
          disposed ||
          manual ||
          version !== stopVersion ||
          loginForm() !== form ||
          accountInput()?.value.trim() !== capture.identifier ||
          passwordInput()?.value !== capture.password
        )
          return;
        savedCredentials = null;
        clearTimeout(deadline);
        deadline = setTimeout(
          () => stop("登录等待超时，请检查官方页面并手动继续。"),
          permission.remainingMs,
        );
        notice("正在恢复统一认证，请稍候…");
      }
      continueOfficialLogin(button);
      continued = true;
      setTimeout(() => {
        allowOfficialSubmit = false;
        submitting = false;
        button.removeAttribute("aria-disabled");
      }, 3000);
    } catch {
      submitting = false;
      button.removeAttribute("aria-disabled");
      if (auto) stop("自动登录未完成，请在官方页面手动继续。");
      else setCaptureError("未能保存本次登录凭据，请刷新后重新输入");
    } finally {
      if (!continued) {
        submitting = false;
        button.removeAttribute("aria-disabled");
      }
    }
  };
  const intercept = (event) => {
    if (allowOfficialSubmit) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (automatic && !manual) stop("已切换为手动登录。");
    void bridgeSubmit();
  };
  const installStartLoginHook = () => {
    const officialStartLogin = window.startLogin;
    if (
      typeof officialStartLogin !== "function" ||
      officialStartLogin.__SZU_LOGIN_CAPTURE_WRAPPED__
    )
      return;
    const guardedStartLogin = function (...args) {
      const target = args[0]?.jquery ? args[0][0] : args[0];
      const passwordFormActive = isActiveFormTarget(document.activeElement);
      const passwordButtonTarget = isActiveLoginButton(target);
      if (allowOfficialSubmit || (!passwordFormActive && !passwordButtonTarget)) {
        return officialStartLogin.apply(this, args);
      }
      if (automatic && !manual) stop("已切换为手动登录。");
      void bridgeSubmit();
    };
    guardedStartLogin.__SZU_LOGIN_CAPTURE_WRAPPED__ = true;
    originalStartLogin = officialStartLogin;
    wrappedStartLogin = guardedStartLogin;
    window.startLogin = guardedStartLogin;
  };
  const interceptEnter = (event) => {
    if (event.isComposing) return;
    const isEnter =
      event.key === "Enter" || event.code === "Enter" || event.keyCode === 13 || event.which === 13;
    const target = isActiveFormTarget(event.target) ? event.target : document.activeElement;
    if (isEnter && isActiveFormTarget(target)) intercept(event);
  };

  const visible = (element) => {
    if (!element || !element.isConnected || !element.getClientRects().length) return false;
    for (let current = element; current instanceof Element; current = current.parentElement) {
      const style = getComputedStyle(current);
      if (current.hidden || style.display === "none" || style.visibility === "hidden") return false;
    }
    return true;
  };
  const blocked = () => {
    if (
      window.needCaptcha === true ||
      [...document.querySelectorAll(".login-main #captchaDiv, #sliderCaptchaDiv")].some(
        (element) =>
          visible(element) && (element.id !== "sliderCaptchaDiv" || element.children.length),
      )
    ) {
      stop("需要完成验证码，请在官方页面手动登录。");
      return true;
    }
    if (
      [
        ...document.querySelectorAll(
          ".login-main .item-error-tip, .login-main .error-tip, .login-main #showErrorTip, .login-main #formErrorTip",
        ),
      ].some((element) => visible(element) && element.textContent.trim())
    ) {
      discardPassword();
      stop("官方页面提示登录异常，请检查后手动继续。");
      return true;
    }
    return false;
  };
  const checkCaptcha = () => {
    if (checking || captchaChecked) return;
    const jq = window.jQuery;
    if (typeof window.checkNeedCaptcha !== "function" || typeof jq !== "function") return;
    checking = true;
    jq(document).on("ajaxComplete.szuAuthRecovery", (_event, xhr, settings) => {
      let path;
      try {
        path = new URL(settings.url, location.href).pathname;
      } catch {
        return;
      }
      if (path !== "/authserver/checkNeedCaptcha.htl") return;
      jq(document).off(".szuAuthRecovery");
      checking = false;
      if (disposed || manual) return;
      if (xhr.status < 200 || xhr.status >= 300 || typeof xhr.responseJSON?.isNeed !== "boolean") {
        stop("验证码检查未完成，请在官方页面手动登录。");
        return;
      }
      captchaChecked = true;
      if (xhr.responseJSON.isNeed) stop("需要完成验证码，请在官方页面手动登录。");
      else void refreshLoginForm();
    });
    try {
      window.checkNeedCaptcha();
    } catch {
      stop("验证码检查未完成，请手动登录。");
    }
    blocked();
  };
  const refreshLoginForm = async () => {
    if (!initialized || disposed || ticking) return;
    ticking = true;
    try {
      disableQrLogin();
      installStartLoginHook();
      if (manual) return;
      if (options.automatic && blocked()) return;
      const form = loginForm();
      const button = loginButton();
      if (
        !visible(form) ||
        !visible(button) ||
        !visible(accountInput()) ||
        !visible(passwordInput()) ||
        accountInput()?.disabled ||
        passwordInput()?.disabled ||
        button.disabled ||
        document.readyState === "loading" ||
        typeof window.startLogin !== "function" ||
        (automatic &&
          !formReady &&
          (typeof window.checkNeedCaptcha !== "function" || typeof window.jQuery !== "function"))
      )
        return;
      if (automatic && !formReady) {
        const status = await control("formReady");
        if (disposed || manual) return;
        if (status.phase !== "ready") {
          stop();
          return;
        }
        formReady = true;
        clearTimeout(deadline);
        deadline = setTimeout(
          () => stop("验证码检查或提交准备超时，请手动继续。"),
          status.remainingMs,
        );
      }
      applySavedCredentials();
      prepare();
      if (!automatic || submitting) return;
      checkCaptcha();
      if (!captchaChecked || manual || blocked()) return;
      // Consume the page opportunity before awaiting IPC; native Claim also
      // persists across replacement documents and failed-login redirects.
      automatic = false;
      await bridgeSubmit(true);
    } catch {
      stop("认证准备未完成，请手动继续。");
    } finally {
      ticking = false;
    }
  };
  listen(
    "click",
    (event) => {
      if (
        !qrAllowed &&
        event.target instanceof Element &&
        event.target.closest(".qrcode-middle, #qrLogin_a, #qrLoginForm")
      ) {
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      if (isActiveLoginButton(event.target)) intercept(event);
    },
    true,
  );
  listen("keydown", interceptEnter, true);
  listen("keypress", interceptEnter, true);
  listen("keyup", interceptEnter, true);
  listen(
    "submit",
    (event) => {
      if (!qrAllowed && event.target instanceof Element && event.target.matches("#qrLoginForm")) {
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      if (isActiveLoginForm(event.target)) {
        if (allowOfficialSubmit) {
          event.preventDefault();
          event.stopImmediatePropagation();
          void submitPasswordForm(event.target);
        } else intercept(event);
      }
    },
    true,
  );
  listen("input", (event) => {
    if (event.isTrusted && !applyingCredentials && isActiveFormTarget(event.target)) {
      discardPassword();
      stop("已切换为手动登录。");
    }
  });
  listen("change", (event) => {
    if (event.isTrusted && !applyingCredentials && isActiveFormTarget(event.target)) {
      discardPassword();
      stop("已切换为手动登录。");
    }
  });
  listen("click", (event) => {
    if (replayingLoginMode || !event.isTrusted) return;
    if (
      event.target instanceof Element &&
      event.target.closest(
        "#pwdLoginSpan, #phoneLoginSpan, #fidoLoginSpan, #qrLoginSpan, #qrLogin_a, #qrCodeA, #qrLoginForm",
      )
    ) {
      event.preventDefault();
      event.stopImmediatePropagation();
      const target = event.target.closest(
        "#pwdLoginSpan, #phoneLoginSpan, #fidoLoginSpan, #qrLoginSpan, #qrLogin_a, #qrCodeA, #qrLoginForm",
      );
      discardPassword();
      stop("已切换为手动登录。");
      void credentialReset.then((accepted) => {
        if (!accepted || disposed || !target?.isConnected) return;
        replayingLoginMode = true;
        try {
          target.click();
        } finally {
          replayingLoginMode = false;
        }
      });
    }
  });
  const unload = () => dispose();
  window.addEventListener("pagehide", unload, { once: true });
  removers.push(() => window.removeEventListener("pagehide", unload));
  void control("status")
    .then((status) => {
      if (disposed) return;
      automatic = status.phase === "loading" || status.phase === "ready";
      formReady = status.phase === "ready";
      // An existing recovery already submitted or handed off on another page.
      if (options.automatic && !automatic) {
        manual = true;
        savedCredentials = null;
        notice(
          status.phase === "submitted"
            ? "本次恢复已尝试登录，请检查官方页面并手动继续。"
            : "自动登录已停止，请在官方页面手动完成登录。",
        );
      }
      if (automatic)
        deadline = setTimeout(
          () => stop("登录页面加载或准备超时，请手动继续。"),
          status.remainingMs,
        );
      initialized = true;
      observer = new MutationObserver(() => {
        void refreshLoginForm();
      });
      observer.observe(document.documentElement, { childList: true, subtree: true });
      interval = setInterval(() => {
        void refreshLoginForm();
      }, 500);
      window.__SZU_LOGIN_CAPTURE_READY__ = true;
      void refreshLoginForm();
    })
    .catch(() => {
      notice("认证自动恢复不可用，请返回应用后重试。");
      dispose();
    });
}
