// Evaluated only by Rust in the main WebView. No credentials are retained in
// this document context: only the one-shot submit invocation receives them.
export default function networkFeePage(options) {
  // This function is serialized and evaluated outside ESM. Strict mode also
  // blocks hostile page getters from reading its credential-bearing arguments.
  "use strict";
  let context;
  const result = (kind) =>
    JSON.stringify({
      kind,
      documentToken: context?.documentToken ?? options.newDocumentToken ?? "uninitialized",
    });

  try {
    if (window.top !== window.self) return result("other");
    const currentHref = window.location.href;
    // Native WebView URLs can change before the old document is unloaded.
    // Ignore that provisional probe without touching its context or form.
    if (options.operation === "probe" && currentHref !== options.expectedUrl) {
      return result("stale");
    }
    context = window.__szuNetworkFeeFlow;
    const current = new URL(currentHref);
    const official =
      current.protocol === "https:" &&
      current.hostname === "self.szu.edu.cn" &&
      (current.port === "" || current.port === "443") &&
      !current.username &&
      !current.password;
    const login = official && currentHref === "https://self.szu.edu.cn/self/nav_login";

    if (options.operation === "cancel") {
      if (context?.generation === options.generation) {
        context.cancelled = true;
        context.manual = true;
        context.dispose?.();
      }
      return result("stale");
    }

    // A delayed probe from an older task cannot replace newer document guards.
    if (context && BigInt(context.generation) > BigInt(options.generation)) return result("stale");

    // Deadline returns are checked in this document too. A manual/Return click
    // wins even when Rust has not yet received the previous probe's callback.
    if (options.operation === "return") {
      if (context && (context.generation !== options.generation || context.cancelled)) {
        return result("stale");
      }
      if (context?.manual) return result("manual");
      context ??= {
        generation: options.generation,
        documentToken: options.newDocumentToken,
      };
      context.cancelled = true;
      context.manual = true;
      window.__szuNetworkFeeFlow = context;
      context.dispose?.();
      window.location.replace(options.returnUrl);
      return result("returning");
    }

    // A credential-bearing invocation must not create context or install any
    // persistent callbacks. It can only act on the exact document we inspected.
    if (options.operation === "submit" || options.operation === "restore") {
      if (
        !context ||
        context.generation !== options.generation ||
        context.documentToken !== options.documentToken ||
        context.cancelled
      ) {
        return result("stale");
      }
      if (context.manual) return result("manual");
      if (!official || (options.operation === "submit" && !login)) return result("invalidForm");
    } else {
      if (!document.documentElement) return result("loading");
      if (!context || context.generation !== options.generation) {
        if (context) {
          context.cancelled = true;
          context.manual = true;
          context.dispose?.();
        }
        context = {
          generation: options.generation,
          documentToken: options.newDocumentToken,
          submitted: false,
          restored: false,
          manual: false,
          cancelled: false,
          domReady: false,
        };
        window.__szuNetworkFeeFlow = context;
        const documentContext = context;
        const root = document.documentElement;
        const toolbar = document.createElement("aside");
        toolbar.id = "szu-network-fee-toolbar";
        const shadow = toolbar.attachShadow({ mode: "open" });
        const style = document.createElement("style");
        style.textContent = [
          ":host{font:14px/1.5 Inter,'Noto Sans SC',system-ui,sans-serif;color:#1f2340;color-scheme:light}",
          "*{box-sizing:border-box}section{display:grid;grid-template-columns:44px minmax(0,1fr) auto;align-items:center;column-gap:12px;row-gap:4px;width:100%;padding:calc(12px + env(safe-area-inset-top,0px)) max(clamp(20px,7vw,112px),env(safe-area-inset-right,0px)) 12px max(clamp(20px,7vw,112px),env(safe-area-inset-left,0px));border-bottom:1px solid #d9dced;background:#f5f6fb;box-shadow:0 8px 24px #25285812}",
          "h2{grid-column:2;grid-row:1;min-width:0;margin:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:clamp(18px,2vw,24px);line-height:1.4;letter-spacing:-.03em}p{grid-column:2/-1;margin:0;color:#69708f;overflow-wrap:anywhere}p.hint{font-size:12px}",
          "a,button{font:inherit;grid-row:1;flex-shrink:0;border-radius:12px;cursor:pointer}a{grid-column:1;display:grid;place-items:center;width:44px;height:44px;color:#69708f;text-decoration:none}a:hover{background:#e9eaff;color:#1f2340}a svg{width:20px;height:20px}button{grid-column:3;min-height:40px;padding:8px 14px;border:1px solid #d9dced;background:white;color:#1f2340;white-space:nowrap}button:hover{background:#eef0ff}",
          "a:focus-visible,button:focus-visible{outline:3px solid #8587ff;outline-offset:2px}section:focus{outline:none}[hidden]{display:none!important}",
          ":host([data-mode=automatic]) p[role=status]::before{content:'';display:inline-block;width:14px;height:14px;margin-right:8px;border:2px solid #dddff5;border-top-color:#5d5fef;border-radius:50%;vertical-align:-2px;animation:spin .8s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}",
          "@media(prefers-reduced-motion:reduce){p[role=status]::before{animation:none!important}}@media(max-width:560px){section{column-gap:8px;padding-left:max(16px,env(safe-area-inset-left,0px));padding-right:max(16px,env(safe-area-inset-right,0px))}h2{font-size:18px}p{grid-column:1/-1}button{padding:8px 12px}}",
        ].join("");
        const section = document.createElement("section");
        section.tabIndex = -1;
        const title = document.createElement("h2");
        title.id = "network-fee-title";
        const message = document.createElement("p");
        message.id = "network-fee-message";
        message.setAttribute("role", "status");
        const hint = document.createElement("p");
        hint.className = "hint";
        hint.textContent =
          "自动处理期间暂时锁定页面，完成后自动解除。如需自行办理，请点击「手动操作」。";
        const back = document.createElement("a");
        back.setAttribute("aria-label", "返回校园卡/电费/网费");
        back.title = "返回校园卡/电费/网费";
        back.href = options.returnUrl;
        back.target = "_self";
        // Match the app's top-left LuArrowLeft CloseButton without loading React
        // or third-party resources into the official page. Keep native navigation.
        const arrow = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        for (const [name, value] of Object.entries({
          viewBox: "0 0 24 24",
          fill: "none",
          stroke: "currentColor",
          "stroke-width": "2",
          "stroke-linecap": "round",
          "stroke-linejoin": "round",
          "aria-hidden": "true",
          focusable: "false",
        }))
          arrow.setAttribute(name, value);
        const arrowPath = document.createElementNS("http://www.w3.org/2000/svg", "path");
        arrowPath.setAttribute("d", "m12 19-7-7 7-7M5 12h14");
        arrow.appendChild(arrowPath);
        back.appendChild(arrow);
        const manual = document.createElement("button");
        manual.type = "button";
        manual.textContent = "手动操作";
        section.append(back, title, manual, message, hint);
        shadow.append(style, section);
        root.appendChild(toolbar);
        documentContext.toolbar = toolbar;
        documentContext.message = message;
        documentContext.manualButton = manual;

        // The bar stays at the top even after Rust hands off. Observe only the
        // head and root's direct children, not form values or the page subtree.
        let offsetBody;
        let originalPadding;
        let originalPaddingPriority;
        let basePadding;
        let offsetHeight;
        const restoreSpacing = () => {
          if (!offsetBody) return;
          offsetBody.style.removeProperty("padding-top");
          if (originalPadding) {
            offsetBody.style.setProperty("padding-top", originalPadding, originalPaddingPriority);
          }
          offsetBody = undefined;
          offsetHeight = undefined;
        };
        const syncSpacing = () => {
          if (documentContext.cancelled) return;
          if (offsetBody !== document.body) {
            restoreSpacing();
            offsetBody = document.body;
            if (offsetBody) {
              originalPadding = offsetBody.style.getPropertyValue("padding-top");
              originalPaddingPriority = offsetBody.style.getPropertyPriority("padding-top");
              const padding = window.getComputedStyle(offsetBody).paddingTop;
              basePadding = !padding || padding === "0" ? "0px" : padding;
            }
          }
          if (!offsetBody) return;
          const height = Math.ceil(section.getBoundingClientRect().height);
          if (height !== offsetHeight) {
            // Keep this longhand independent of any existing padding shorthand.
            offsetBody.style.removeProperty("padding-top");
            offsetBody.style.setProperty(
              "padding-top",
              "calc(" + basePadding + " + " + height + "px)",
              "important",
            );
            offsetHeight = height;
          }
        };
        const syncTitle = () => {
          if (documentContext.cancelled) return;
          const text = document.title.trim() || "网费服务";
          // The official title is untrusted text, never markup or a destination.
          if (title.textContent !== text) {
            title.textContent = text;
            title.title = text;
          }
          syncSpacing();
        };
        const titleObserver = new MutationObserver(syncTitle);
        let observedHead;
        const syncLayout = () => {
          if (documentContext.cancelled) return;
          if (!toolbar.isConnected) root.appendChild(toolbar);
          if (observedHead !== document.head) {
            titleObserver.disconnect();
            observedHead = document.head;
            if (observedHead)
              titleObserver.observe(observedHead, {
                childList: true,
                subtree: true,
                characterData: true,
              });
          }
          syncTitle();
        };
        const layoutObserver = new MutationObserver(syncLayout);
        layoutObserver.observe(root, { childList: true });
        const resizeObserver =
          typeof ResizeObserver === "function" ? new ResizeObserver(syncSpacing) : undefined;
        resizeObserver?.observe(section);
        window.addEventListener("resize", syncSpacing);

        let locked = false;
        let lockedBody;
        let originalInert;
        let originalOverflow;
        let originalFocus;
        const restoreBody = () => {
          if (!lockedBody) return;
          if (originalInert === null) lockedBody.removeAttribute("inert");
          else lockedBody.setAttribute("inert", originalInert);
          lockedBody = undefined;
        };
        const syncBody = () => {
          if (!locked || documentContext.manual || documentContext.cancelled) return;
          if (lockedBody !== document.body) {
            restoreBody();
            lockedBody = document.body;
            if (lockedBody) {
              originalInert = lockedBody.getAttribute("inert");
              lockedBody.setAttribute("inert", "");
            }
          }
          // Body replacement must not discard the mask. The host is deliberately
          // outside body so inert never disables the Manual/Return controls.
          if (!toolbar.isConnected) root.appendChild(toolbar);
        };
        const observer = new MutationObserver(syncBody);
        const release = (restoreFocus = true) => {
          if (!locked) return;
          locked = false;
          observer.disconnect();
          restoreBody();
          root.style.removeProperty("overflow");
          for (const [property, value, priority] of originalOverflow) {
            root.style.setProperty(property, value, priority);
          }
          if (restoreFocus && document.activeElement === toolbar) {
            if (originalFocus?.isConnected && originalFocus !== document.body) {
              originalFocus.focus({ preventScroll: true });
            } else {
              back.focus({ preventScroll: true });
            }
          }
        };
        const lock = () => {
          if (locked) return;
          locked = true;
          originalFocus = document.activeElement;
          // Save longhands too: setting the shorthand can erase overflow-x/y.
          originalOverflow = Array.from(root.style)
            .filter((property) => /^overflow(?:-[xy])?$/.test(property))
            .map((property) => [
              property,
              root.style.getPropertyValue(property),
              root.style.getPropertyPriority(property),
            ]);
          root.style.setProperty("overflow", "hidden", "important");
          syncBody();
          observer.observe(root, { childList: true });
          section.focus({ preventScroll: true });
        };
        documentContext.updateToolbar = (text) => {
          if (text !== undefined && !documentContext.userTakeover && message.textContent !== text)
            message.textContent = text;
          const automatic = !documentContext.manual;
          const mode = automatic ? "automatic" : "manual";
          if (toolbar.dataset.mode !== mode) {
            toolbar.dataset.mode = mode;
            toolbar.style.cssText =
              "all:initial!important;position:fixed!important;z-index:2147483647!important;box-sizing:border-box!important;pointer-events:auto!important;display:block!important;font:14px/1.5 Inter,'Noto Sans SC',system-ui,sans-serif!important;color:#1f2340!important;" +
              (automatic
                ? "inset:0px!important;overflow:auto!important;overscroll-behavior:contain!important;background:rgba(20,24,45,.48)!important;"
                : "inset:0px 0px auto!important;");
            hint.hidden = !automatic;
            manual.hidden = !automatic;
            section.setAttribute("role", automatic ? "dialog" : "region");
            section.setAttribute("aria-labelledby", title.id);
            section.setAttribute("aria-describedby", message.id);
            if (automatic) section.setAttribute("aria-modal", "true");
            else section.removeAttribute("aria-modal");
          }
          if (automatic) lock();
          else release();
          syncTitle();
        };
        manual.addEventListener("click", () => {
          // Invalidate synchronously: even an already queued submit/restore or
          // timeout return must yield before Rust receives the next probe.
          documentContext.manual = true;
          documentContext.userTakeover = true;
          message.textContent = "已停止自动操作，请自行办理；应用不会自动充值、扣款或修改套餐。";
          documentContext.updateToolbar();
        });
        const invalidate = () => {
          documentContext.cancelled = true;
          documentContext.manual = true;
          documentContext.dispose();
        };
        back.addEventListener("click", () => {
          // Leave the link connected until its native navigation runs.
          documentContext.cancelled = true;
          documentContext.manual = true;
          documentContext.updateToolbar();
        });
        window.addEventListener("pagehide", invalidate);

        // Do not equate readyState=interactive with DOM ready: deferred scripts
        // and later DOMContentLoaded listeners can still reveal a captcha.
        // This callback is installed only by credential-free probe/toolbar evals.
        let readyTimer;
        const onDomReady = () => {
          readyTimer = window.setTimeout(() => {
            if (!documentContext.cancelled) documentContext.domReady = true;
          }, 0);
        };
        document.addEventListener("DOMContentLoaded", onDomReady, { once: true });

        const blockInteraction = (event) => {
          if (!event.isTrusted || !locked || documentContext.manual || documentContext.cancelled)
            return;
          const inside = event.target === toolbar || event.composedPath?.().includes(toolbar);
          if (event.type === "keydown" && event.key === "Tab") {
            event.preventDefault();
            event.stopImmediatePropagation();
            const active = shadow.activeElement;
            (active === back || (active !== manual && event.shiftKey) ? manual : back).focus({
              preventScroll: true,
            });
          } else if (!inside || (event.type === "keydown" && event.key === "Escape")) {
            event.preventDefault();
            event.stopImmediatePropagation();
            if (event.type === "focusin") section.focus({ preventScroll: true });
          }
        };
        // No submit/invalid guard: the native click below can legitimately emit
        // a trusted submit or validation event. Synthetic input/change is allowed.
        const blockedEvents = [
          "pointerdown",
          "pointerup",
          "mousedown",
          "mouseup",
          "click",
          "dblclick",
          "touchstart",
          "touchmove",
          "touchend",
          "wheel",
          "keydown",
          "keyup",
          "keypress",
          "beforeinput",
          "input",
          "change",
          "focusin",
          "contextmenu",
          "dragstart",
          "drop",
        ];
        for (const type of blockedEvents) {
          document.addEventListener(type, blockInteraction, { capture: true, passive: false });
        }
        documentContext.dispose = () => {
          release(false);
          layoutObserver.disconnect();
          titleObserver.disconnect();
          resizeObserver?.disconnect();
          window.removeEventListener("resize", syncSpacing);
          restoreSpacing();
          window.clearTimeout(readyTimer);
          document.removeEventListener("DOMContentLoaded", onDomReady);
          window.removeEventListener("pagehide", invalidate);
          for (const type of blockedEvents)
            document.removeEventListener(type, blockInteraction, true);
          toolbar.remove();
        };
        syncLayout();
      }
      if (context.cancelled) return result("stale");
      if (options.manual) context.manual = true;
      if (!context.toolbar.isConnected) document.documentElement.appendChild(context.toolbar);
      // A late automatic probe must neither re-lock nor replace manual copy.
      context.updateToolbar(!context.manual || options.manual ? options.message : undefined);
      if (options.operation === "toolbar") return result("toolbar");
      if (context.manual) return result("manual");
    }

    const visible = (element) => {
      if (!element?.isConnected || element.hidden || !element.getClientRects().length) return false;
      for (let node = element; node instanceof Element; node = node.parentElement) {
        const style = window.getComputedStyle(node);
        if (node.hidden || style.display === "none" || style.visibility === "hidden") return false;
      }
      return true;
    };
    const hasError = () =>
      [...document.querySelectorAll("#fielderror1, #fielderror2")].some((node) =>
        node.textContent.trim(),
      );
    const hasCaptcha = () =>
      [
        ...document.querySelectorAll("#tr_random, #radomcode, #codeimage, [name*='captcha' i]"),
      ].some(visible);
    const inspectForm = () => {
      const forms = document.querySelectorAll("#loginform");
      if (forms.length === 0) return { kind: "pending" };
      if (forms.length !== 1 || !(forms[0] instanceof HTMLFormElement))
        return { kind: "invalidForm" };
      const form = forms[0];
      const accounts = form.querySelectorAll("#account");
      const passwords = form.querySelectorAll("#pass");
      const buttons = form.querySelectorAll(
        "input[type='submit'], button[type='submit'], button:not([type])",
      );
      if (accounts.length !== 1 || passwords.length !== 1 || buttons.length !== 1)
        return { kind: "invalidForm" };
      const account = accounts[0];
      const password = passwords[0];
      const button = buttons[0];
      const action = new URL(form.getAttribute("action"), document.baseURI);
      if (
        form.method.toLowerCase() !== "post" ||
        (form.target && form.target !== "_self") ||
        action.origin !== "https://self.szu.edu.cn" ||
        action.pathname !== "/self/LoginAction.action" ||
        action.username ||
        action.password ||
        action.search ||
        action.hash ||
        !(account instanceof HTMLInputElement) ||
        account.type !== "text" ||
        account.name !== "account" ||
        !(password instanceof HTMLInputElement) ||
        password.type !== "password" ||
        password.name !== "password" ||
        [account, password, button].some(
          (element) => element.form !== form || element.disabled || !visible(element),
        ) ||
        account.readOnly ||
        password.readOnly ||
        ["formaction", "formmethod", "formtarget", "formenctype", "formnovalidate"].some(
          (attribute) => button.hasAttribute(attribute),
        )
      )
        return { kind: "invalidForm" };
      if (hasError()) return { kind: "loginError" };
      if (hasCaptcha()) return { kind: "captcha" };
      return { kind: "loginReady", account, password, button, form };
    };

    if (!official) return result("other");
    // DOM-ready is sufficient; slow images and unrelated async scripts must not
    // delay login. Timing covers probes installed after DOMContentLoaded fired;
    // the credential-free listener above covers engines without that API.
    let domReady = document.readyState === "complete" || context.domReady;
    if (!domReady) {
      try {
        domReady =
          window.performance.getEntriesByType?.("navigation")[0]?.domContentLoadedEventEnd > 0 ||
          window.performance.timing?.domContentLoadedEventEnd > 0;
      } catch {
        // Missing timing evidence is not permission to race DOM-ready handlers.
      }
    }
    if (!domReady) return result("loading");
    if (hasError()) return result("loginError");
    if (login && hasCaptcha()) return result("captcha");
    if (!login) {
      if (
        document.querySelector("#loginform, #account, #pass") ||
        current.pathname === "/self/nav_login"
      ) {
        return result("invalidForm");
      }
      // The anonymous portal returns this inline redirect as a complete HTML
      // document before nav_login commits. It is not an authenticated portal.
      const loginRedirect = [...document.scripts].some((script) => {
        if (script.hasAttribute("src")) return false;
        const match = script.textContent
          .trim()
          .match(/^window\.location\.href\s*=\s*(["'])(.*?)\1\s*;?$/);
        return (
          match &&
          new URL(match[2], document.baseURI).href === "https://self.szu.edu.cn/self/nav_login"
        );
      });
      if (loginRedirect) return result("pending");
      if (options.operation === "restore") {
        if (context.restored) return result("stale");
        context.restored = true;
        window.location.replace("https://self.szu.edu.cn/self/nav_operators");
      }
      return result("portal");
    }
    if (options.operation === "restore") return result("loginError");
    const form = inspectForm();
    if (form.kind !== "loginReady") return result(form.kind);
    if (context.submitted) return result("submitted");
    if (form.account.value || form.password.value) {
      context.manual = true;
      context.updateToolbar();
      return result("manual");
    }
    if (options.operation !== "submit") return result("loginReady");
    if (
      typeof options.account !== "string" ||
      !options.account ||
      typeof options.password !== "string" ||
      !options.password
    ) {
      return result("invalidForm");
    }

    // Reserve before any input event or native click; a lost native callback
    // must never cause another automatic attempt on this document.
    context.submitted = true;
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setValue.call(form.account, options.account);
    setValue.call(form.password, options.password);
    for (const input of [form.account, form.password]) {
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    }
    // Input handlers may have replaced the form, enabled captcha, or changed
    // its destination. Revalidate before clicking the official submit control.
    const rechecked = inspectForm();
    if (
      context.cancelled ||
      context.manual ||
      window.location.href !== currentHref ||
      rechecked.kind !== "loginReady" ||
      rechecked.form !== form.form ||
      rechecked.account !== form.account ||
      rechecked.password !== form.password ||
      rechecked.button !== form.button ||
      form.account.value !== options.account ||
      form.password.value !== options.password
    ) {
      setValue.call(form.account, "");
      setValue.call(form.password, "");
      context.manual = true;
      context.updateToolbar();
      return result(rechecked.kind === "captcha" ? "captcha" : "invalidForm");
    }
    HTMLElement.prototype.click.call(form.button);
    return result("submitted");
  } catch {
    return result("invalidForm");
  }
}
