import { mkdir, readFile, writeFile } from "node:fs/promises";
const fixture = await readFile(
  new URL("../testing/android/ui-fixture.js", import.meta.url),
  "utf8",
);
const port = process.argv[2] || 18423,
  out = process.argv[3] || "./.android-test/reports/ui-layout",
  base = process.argv[4] || "http://127.0.0.1:18422";
if (!["127.0.0.1", "localhost", "[::1]"].includes(new URL(base).hostname)) {
  throw Error("Use a local preview of the built frontend with isolated test data.");
}
await mkdir(out, { recursive: true });
const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const ws = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((r, j) => {
  ws.addEventListener("open", r, { once: true });
  ws.addEventListener("error", j, { once: true });
});
let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => {
  const v = JSON.parse(e.data);
  if (v.id) {
    const c = pending.get(v.id);
    pending.delete(v.id);
    if (v.error) c.reject(Error(JSON.stringify(v.error)));
    else c.resolve(v.result);
  }
});
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => {
    const key = ++id;
    pending.set(key, { resolve, reject });
    ws.send(JSON.stringify({ id: key, method, params, sessionId }));
  });
const delay = (ms) => new Promise((r) => setTimeout(r, ms)),
  reports = [];
const contexts = new Set();
const specs = [
  ["home", 360, 711],
  ["home-small", 320, 568],
  ["home-wide", 1200, 900],
  ["home-dark", 360, 711],
  ["home-large-font", 320, 568],
  ["auth-keyboard", 360, 320],
  ["dormitory", 360, 711],
  ["dorm-small", 320, 568],
  ["dorm-landscape", 800, 360],
  ["auth", 360, 711],
  ["storage", 360, 711],
  ["appearance", 360, 711],
  ["local_model", 360, 711],
  ["ai", 360, 711],
  ["mcp", 360, 711],
  ["study", 360, 711],
  ["life", 360, 711],
  ["settings", 360, 711],
  ["home-todos", 360, 711],
  ["home-todos-dark", 360, 711],
  ["payments-entry", 360, 711],
  ["payments-card", 320, 568],
  ["payments-network", 360, 711],
  ["payments-electricity", 360, 711],
  ["payments-recharge", 360, 711],
];
try {
  for (const [name, width, height] of specs) {
    const scenario = name.startsWith("auth")
      ? "auth"
      : name.startsWith("home")
        ? "home"
        : name.startsWith("dorm")
          ? "dormitory"
          : name;
    const { browserContextId } = await send("Target.createBrowserContext");
    contexts.add(browserContextId);
    const { targetId } = await send("Target.createTarget", {
      url: "about:blank",
      browserContextId,
    });
    const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
    const cmd = (m, p = {}) => send(m, p, sessionId),
      evaluate = async (expression) => {
        const r = await cmd("Runtime.evaluate", {
          expression,
          returnByValue: true,
          awaitPromise: true,
        });
        if (r.exceptionDetails) throw Error(JSON.stringify(r.exceptionDetails));
        return r.result.value;
      };
    const click = async (point) => {
      if (!point || point.x < 0 || point.y < 0 || point.x >= width || point.y >= height)
        throw Error(`${name}: click target is outside the viewport`);
      await cmd("Input.dispatchMouseEvent", {
        type: "mousePressed",
        ...point,
        button: "left",
        clickCount: 1,
      });
      await cmd("Input.dispatchMouseEvent", {
        type: "mouseReleased",
        ...point,
        button: "left",
        clickCount: 1,
      });
    };
    await cmd("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: true,
    });
    const route = scenario.startsWith("payments-")
      ? scenario === "payments-entry"
        ? "/life"
        : "/life/campus-payments/" + scenario.slice(9)
      : ["study", "life", "settings"].includes(scenario)
        ? "/" + scenario
        : "/";
    await cmd("Page.enable");
    await cmd("Page.addScriptToEvaluateOnNewDocument", { source: fixture });
    await cmd("Page.navigate", {
      url: base + route + "?scenario=" + scenario + (name.endsWith("dark") ? "&dark=1" : ""),
    });
    let ready = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      ready = await evaluate(
        `!!window.__szuUiFixture && !!document.querySelector('.page-surface') && ${
          scenario === "home"
            ? "!!document.querySelector('[role=grid][aria-label=月历]')"
            : ["auth", "storage", "dormitory", "appearance", "local_model", "ai", "mcp"].includes(
                  scenario,
                )
              ? "!!document.querySelector('.onboarding-actions m-button')"
              : "!!document.querySelector('.top-app-bar')"
        }`,
      );
      if (ready) break;
      await delay(100);
    }
    if (!ready) throw Error(`${name}: frontend did not become ready within 10 seconds`);
    await evaluate("document.fonts.ready.then(() => true)");
    await delay(200);
    if (scenario === "payments-entry") {
      await click(
        await evaluate(
          `(() => {const b=[...document.querySelectorAll('m-button,button')].find(e=>e.textContent.includes('校园卡/电费/网费'));const r=b.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`,
        ),
      );
      await delay(500);
    }
    if (["payments-entry", "payments-electricity"].includes(scenario)) {
      for (let attempt = 0; attempt < 100; attempt++) {
        if (
          await evaluate(
            `!!document.querySelector('.recharts-wrapper svg.recharts-surface') || document.body.innerText.includes('Something went wrong!')`,
          )
        )
          break;
        await delay(100);
      }
    } else if (scenario.startsWith("payments-")) await delay(400);
    if (name === "home-large-font")
      await evaluate('document.documentElement.style.fontSize="24px"');
    if (scenario === "settings") {
      await evaluate(`document.querySelector('.adaptive-tabs-trigger')?.click()`);
      await delay(200);
    }
    if (scenario === "dormitory") {
      await evaluate(
        `(() => {const s=[...document.querySelectorAll('m-select')].find(e=>e.getAttribute('label')==='楼栋');s?.scrollIntoView({behavior:'instant',block:'center'});return !!s})()`,
      );
      await delay(200);
      await click(
        await evaluate(
          `(() => {const s=[...document.querySelectorAll('m-select')].find(e=>e.getAttribute('label')==='楼栋');const r=s.shadowRoot.querySelector('.mtrl-select').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`,
        ),
      );
      await delay(350);
    }
    const result = await evaluate(`(() => {
   const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height,right:r.right,bottom:r.bottom}};
   function walk(root){return [...root.querySelectorAll('*')].flatMap(e=>[e,...(e.shadowRoot?walk(e.shadowRoot):[])])}
   const all=walk(document),list=all.find(e=>e.getAttribute('role')==='listbox'&&e.getBoundingClientRect().height>0);
   const footer=document.querySelector('.onboarding-actions'), nav=document.querySelector('.app-nav-bar');
   const overflow=[...document.querySelectorAll('.page-surface-inner,m-card,m-text-field,m-select,.top-app-bar')].filter(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e);return r.width>0&&(r.right>innerWidth+1||r.left<-1)&&s.position!=='absolute'&&s.visibility!=='hidden'&&s.display!=='none'&&!e.closest('m-menu,m-menu-item')}).slice(0,15).map(e=>({tag:e.localName,class:e.className,rect:rect(e)}));
   const calendar=document.querySelector('[role=grid][aria-label=月历]');
   const day=calendar?.querySelector('[aria-selected=true]');
   const login=[...document.querySelectorAll('m-button')].find(e=>e.textContent.trim()==='登录并继续'),loginRect=login?.getBoundingClientRect();
   const chips=[...document.querySelectorAll('.home-calendar-todo .md-chip')].map(e=>({text:e.textContent,color:getComputedStyle(e).color,background:getComputedStyle(e).backgroundColor}));
   return {name: ${JSON.stringify(name)},browser:navigator.userAgent,width:innerWidth,height:innerHeight,text:document.body.innerText.slice(0,400),errors:window.__szuUiFixture?.errors,scrollWidth:document.documentElement.scrollWidth,overflow,footer:footer&&rect(footer),nav:nav&&rect(nav),chips,charts:document.querySelectorAll('.recharts-wrapper svg.recharts-surface').length,hasOwn:typeof Object.hasOwn,loginClickable:!!loginRect&&loginRect.bottom<=innerHeight&&document.elementFromPoint(loginRect.x+loginRect.width/2,loginRect.y+loginRect.height/2)===login,list:list&&{rect:rect(list),options:[...list.querySelectorAll('[role=option]')].slice(0,3).map(e=>({label:e.textContent,rect:rect(e),hit:(list.getRootNode().elementFromPoint?.(e.getBoundingClientRect().x+10,e.getBoundingClientRect().y+10)||document.elementFromPoint(e.getBoundingClientRect().x+10,e.getBoundingClientRect().y+10))?.localName}))},calendar:calendar&&{rect:rect(calendar),columns:getComputedStyle(calendar).gridTemplateColumns},selected:day&&{foreground:getComputedStyle(day).color,background:getComputedStyle(day).backgroundColor}};
  })()`);
    const screenshot = await cmd("Page.captureScreenshot", { format: "png" });
    await writeFile(`${out}/${name}.png`, Buffer.from(screenshot.data, "base64"));
    if (name.startsWith("home-todos")) {
      await evaluate(
        `document.querySelector('.home-calendar-todos').scrollIntoView({behavior:'instant',block:'start'})`,
      );
      await delay(150);
      await writeFile(
        `${out}/${name}.png`,
        Buffer.from((await cmd("Page.captureScreenshot", { format: "png" })).data, "base64"),
      );
    }
    reports.push(result);
    console.log(
      JSON.stringify({
        name,
        overflow: result.overflow.length,
        scrollWidth: result.scrollWidth,
        errors: result.errors,
        list: result.list?.rect,
        calendar: result.calendar?.columns,
      }),
    );
    if (scenario === "dormitory" && result.list) {
      await click(
        await evaluate(
          `(() => {const s=[...document.querySelectorAll('m-select')].find(e=>e.getAttribute('label')==='楼栋');const option=s.shadowRoot.querySelector('[role=option][data-id="2"]');option.scrollIntoView({behavior:'instant',block:'nearest'});const r=option.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`,
        ),
      );
      await delay(100);
      result.selection = await evaluate(
        `(() => {const s=[...document.querySelectorAll('m-select')].find(e=>e.getAttribute('label')==='楼栋');return {value:s.value,open:s.shadowRoot.querySelector('.mtrl-menu')?.classList.contains('mtrl-menu--visible')}})()`,
      );
    }
    if (scenario === "home") {
      await evaluate(
        `document.querySelector('[role=grid][aria-label=月历] button').scrollIntoView({behavior:'instant',block:'center'})`,
      );
      await delay(100);
      await click(
        await evaluate(
          `(() => {const day=document.querySelector('[role=grid][aria-label=月历] button');const r=day.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`,
        ),
      );
      await delay(100);
      result.calendarSelection = await evaluate(
        `(() => {const day=document.querySelector('[role=grid][aria-label=月历] button');return day?{selected:day.getAttribute('aria-selected'),color:getComputedStyle(day).color,background:getComputedStyle(day).backgroundColor}:null})()`,
      );
    }
    if (scenario === "auth") {
      await click(
        await evaluate(
          `(() => {const b=[...document.querySelectorAll('m-button')].find(e=>e.textContent.trim()==='登录并继续');const r=b.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`,
        ),
      );
      await delay(200);
      result.validationReached = await evaluate(
        `Array.from(document.querySelectorAll('m-snackbar')).some(e=>(e.getAttribute('message')||'').includes('校园卡号必须是 6 位数字'))`,
      );
    }
    if (scenario === "settings" && result.list) {
      await click(
        await evaluate(
          `(() => {const option=[...document.querySelectorAll('.adaptive-tabs-popover [role=option]')].find(e=>e.textContent.trim()==='外观');const r=option.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`,
        ),
      );
      await delay(300);
      result.tabSelection = await evaluate(
        `document.querySelector('[role=tabpanel]')?.getAttribute('aria-label')==='外观' && !document.querySelector('.adaptive-tabs-popover')`,
      );
    }
    const pageReady = !result.text.includes("Something went wrong!") && !!result.text.trim();
    result.passed =
      pageReady &&
      result.errors.length === 0 &&
      result.scrollWidth <= width &&
      result.overflow.length === 0;
    if (scenario === "home")
      result.passed &&=
        !!result.calendar &&
        result.calendar.columns.split(" ").length === 7 &&
        result.calendarSelection?.selected === "true" &&
        result.calendarSelection.color !== result.calendarSelection.background &&
        result.calendarSelection.background !== "rgba(0, 0, 0, 0)";
    if (scenario === "home")
      result.passed &&=
        result.chips.length === 6 &&
        result.chips.every(
          (c) => c.text.trim() && c.color !== c.background && c.background !== "rgba(0, 0, 0, 0)",
        );
    if (scenario.startsWith("payments-")) {
      result.passed &&= result.text.includes("校园卡/电费/网费") && result.hasOwn === "function";
      if (["payments-entry", "payments-electricity"].includes(scenario))
        result.passed &&= result.charts > 0;
    }
    if (scenario === "dormitory")
      result.passed &&=
        !!result.list &&
        result.list.rect.right <= width &&
        result.list.rect.y >= 0 &&
        result.list.rect.bottom <= height &&
        result.selection?.value === "2" &&
        !result.selection.open;
    if (result.footer) result.passed &&= result.footer.bottom <= height && !result.nav;
    if (scenario === "auth") result.passed &&= result.loginClickable && result.validationReached;
    if (scenario === "settings")
      result.passed &&=
        !!result.list && result.list.rect.bottom <= result.nav.y && result.tabSelection;
    await send("Target.closeTarget", { targetId });
    await send("Target.disposeBrowserContext", { browserContextId });
    contexts.delete(browserContextId);
  }
  await writeFile(
    `${out}/results.json`,
    JSON.stringify({ browser: version.Browser, reports }, null, 2),
  );
} finally {
  await Promise.allSettled(
    [...contexts].map((browserContextId) =>
      send("Target.disposeBrowserContext", { browserContextId }),
    ),
  );
  ws.close();
}
const failed = reports.filter((r) => !r.passed).map((r) => r.name);
if (failed.length) throw Error("Layout checks failed: " + failed.join(", "));
console.log(`PASS: ${reports.length} layout scenarios on ${version.Browser}`);
