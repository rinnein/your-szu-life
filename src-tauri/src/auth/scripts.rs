use super::{model::LoginFormPrefill, AUTH_HOST};

pub(super) fn auth_probe_script() -> &'static str {
    r#"(()=>{window.__SZU_AUTH_PROBE__=null;(async()=>{try{const r=await fetch('/personalInfo/common/getUserConf',{credentials:'include'});const t=await r.text();window.__SZU_AUTH_PROBE__={ok:r.ok,status:r.status,url:r.url,text:t}}catch(e){window.__SZU_AUTH_PROBE__={ok:false,error:String(e)}}})();return null})()"#
}

pub(super) fn auth_probe_result_script() -> &'static str {
    "window.__SZU_AUTH_PROBE__"
}

pub(super) fn auth_login_capture_script(
    allow_qr_login: bool,
    prefill: Option<&LoginFormPrefill>,
) -> String {
    let saved_credentials = prefill.map_or_else(
        || "null".to_owned(),
        |credentials| serde_json::to_string(credentials).expect("login prefill is serializable"),
    );
    format!(
        r#"
(() => {{
  if (location.protocol !== 'https:' || location.host !== '{AUTH_HOST}' ||
      !location.pathname.startsWith('/authserver/login') || window.__SZU_LOGIN_CAPTURE_INSTALLED__) return;
  window.__SZU_LOGIN_CAPTURE_INSTALLED__ = true;

  const qrAllowed = {allow_qr_login};
  const savedCredentials = {saved_credentials};
  const autofilledForms = new WeakSet();
  let submitting = false;
  let allowOfficialSubmit = false;
  let candidateCapture = null;

  // The official page retains hidden templates with duplicated IDs. Only the
  // form copied into .login-main is live, so never fall back to global fields.
  // Desktop uses .m-account while mobile uses .pwd_login; querying from the
  // active form supports both layouts without crossing into hidden templates.
  const loginForm = () => document.querySelector('.login-main #pwdFromId');
  const formField = selector => loginForm()?.querySelector(selector);
  const accountInput = () => formField('#username');
  const passwordInput = () => formField('#password');
  const loginButton = () => formField('#login_submit');
  const rememberInput = () => formField('#rememberMe');
  const isActiveFormTarget = target => {{
    const form = loginForm();
    return !!form && target instanceof Node && form.contains(target);
  }};
  const isActiveLoginButton = target => {{
    if (!(target instanceof Element)) return false;
    const button = target.closest('.login-main #pwdFromId #login_submit');
    return !!button && button === loginButton();
  }};
  const setCaptureError = message => {{
    const error = document.querySelector('.login-main #showErrorTip, .login-main #pwdErrorTip, .login-main #formErrorTip, .login-main .m-account .error-tip, .login-main .pwd_login .error-tip');
    if (error) {{
      error.textContent = message;
      error.classList.remove('hide');
      error.style.display = 'block';
    }}
  }};
  const applySavedCredentials = () => {{
    if (!savedCredentials) return;
    const form = loginForm();
    const account = accountInput();
    const password = passwordInput();
    if (!form || !account || !password || autofilledForms.has(form)) return;
    // The values are only injected into the active account/password form on
    // the HTTPS official login origin. Mark this specific form so the user can
    // still deliberately edit or clear either field afterwards.
    account.value = savedCredentials.identifier;
    password.value = savedCredentials.password;
    for (const input of [account, password]) {{
      input.dispatchEvent(new Event('input', {{ bubbles: true }}));
      input.dispatchEvent(new Event('change', {{ bubbles: true }}));
    }}
    autofilledForms.add(form);
  }};
  const disableQrLogin = () => {{
    if (qrAllowed) return;
    for (const area of document.querySelectorAll('.qrcode-middle')) {{
      area.setAttribute('aria-disabled', 'true');
      if (getComputedStyle(area).position === 'static') area.style.position = 'relative';
      if (area.querySelector('[data-szu-app-qr-disabled]')) continue;
      const overlay = document.createElement('div');
      overlay.dataset.szuAppQrDisabled = 'true';
      overlay.setAttribute('role', 'note');
      overlay.textContent = '二维码登录已禁用\n请使用学号/校园卡号和密码登录，以便完成账户设置';
      Object.assign(overlay.style, {{
        position: 'absolute', inset: '0', zIndex: '2147483647', display: 'flex',
        alignItems: 'center', justifyContent: 'center', padding: '20px', boxSizing: 'border-box',
        textAlign: 'center', whiteSpace: 'pre-line', color: '#6b2b2b', background: 'rgba(255,248,248,.97)',
        border: '1px solid #e6b6b6', borderRadius: '6px', fontSize: '14px', lineHeight: '1.6',
        fontWeight: '600', cursor: 'not-allowed', pointerEvents: 'auto'
      }});
      area.append(overlay);
    }}
  }};
  const prepare = () => {{
    disableQrLogin();
    const form = loginForm();
    const account = accountInput();
    const password = passwordInput();
    const button = loginButton();
    const remember = rememberInput();
    if (!form || !account || !password || !button) return null;
    if (remember && !remember.checked) {{
      remember.checked = true;
      remember.dispatchEvent(new Event('input', {{ bubbles: true }}));
      remember.dispatchEvent(new Event('change', {{ bubbles: true }}));
    }}
    if (!account.value.trim() || !password.value) return null;
    return {{ identifier: account.value.trim(), password: password.value }};
  }};
  const captureCandidate = async () => {{
    const capture = prepare();
    if (!capture) return false;
    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (typeof invoke !== 'function') return false;
    const key = `${{capture.identifier}}\u0000${{capture.password}}`;
    if (candidateCapture?.key === key) return candidateCapture.promise;
    const promise = invoke('capture_auth_credentials', {{ capture }}).then(() => true).catch(() => false);
    candidateCapture = {{ key, promise }};
    const captured = await promise;
    if (!captured && candidateCapture?.key === key) candidateCapture = null;
    return captured;
  }};
  const continueOfficialLogin = button => {{
    if (!button || button !== loginButton() || typeof window.startLogin !== 'function') {{
      throw new Error('active login button or startLogin unavailable');
    }}
    allowOfficialSubmit = true;
    window.startLogin(button);
  }};
  const bridgeSubmit = async () => {{
    if (submitting) return;
    const capture = prepare();
    const button = loginButton();
    if (!capture || !button) {{
      setCaptureError('未能读取当前账号密码表单，请刷新后重新输入');
      return;
    }}
    submitting = true;
    button.setAttribute('aria-disabled', 'true');
    try {{
      if (!await captureCandidate()) throw new Error('credential capture failed');
      continueOfficialLogin(button);
      setTimeout(() => {{ allowOfficialSubmit = false; submitting = false; button.removeAttribute('aria-disabled'); }}, 3000);
    }} catch (_) {{
      submitting = false;
      button.removeAttribute('aria-disabled');
      setCaptureError('未能保存本次登录凭据，请刷新后重新输入');
    }}
  }};
  const intercept = event => {{
    if (allowOfficialSubmit) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    void bridgeSubmit();
  }};
  const interceptEnter = event => {{
    if ((event.key === 'Enter' || event.keyCode === 13) && isActiveFormTarget(event.target)) intercept(event);
  }};

  applySavedCredentials();
  prepare();
  const refreshLoginForm = () => {{ applySavedCredentials(); prepare(); }};
  new MutationObserver(refreshLoginForm).observe(document.documentElement, {{ childList: true, subtree: true }});
  setInterval(refreshLoginForm, 500);
  document.addEventListener('click', event => {{
    if (!qrAllowed && event.target instanceof Element && event.target.closest('.qrcode-middle, #qrLogin_a, #qrLoginForm')) {{
      event.preventDefault(); event.stopImmediatePropagation(); return;
    }}
    if (isActiveLoginButton(event.target)) intercept(event);
  }}, true);
  document.addEventListener('keydown', interceptEnter, true);
  document.addEventListener('keyup', interceptEnter, true);
  document.addEventListener('submit', event => {{
    if (!qrAllowed && event.target instanceof Element && event.target.matches('#qrLoginForm')) {{
      event.preventDefault(); event.stopImmediatePropagation(); return;
    }}
    if (event.target === loginForm()) intercept(event);
  }}, true);
}})()
"#
    )
}
