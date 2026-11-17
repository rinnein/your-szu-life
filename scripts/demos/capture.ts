/**
 * 用演示模式批量生成 README 演示图。
 *
 * 演示模式下 `invoke` 被 vite alias 改道到 `src/demo/shim.ts`，业务代码读到
 * 的都是 `src/demo/fixtures/*` 的假数据；纯浏览器只靠以下 localStorage 键
 * 就能进入「已登录 + 有数据」的状态，不需要真机与真实账号：
 *  - `demo.enabled=true`：决定 shim 是否接管，`ThemeProvider.isTauri()` 也认它；
 *  - `szu.appearance-mode`：首帧即生效的深浅色，避免截图时闪一下主题。
 *
 * 所有截图先按 2 倍像素密度落到系统临时目录，再经 ImageMagick 一次性降采样
 * 并转 WebP 写进 `demos/`——入库的是 webp，临时 PNG 不会残留。
 *
 * 用法：
 *   vpx scripts/demos/capture.ts                # 全部场景 + 展示页
 *   vpx scripts/demos/capture.ts --only uooc    # 只重截某一张
 *   vpx scripts/demos/capture.ts --skip-build   # 复用已有 dist/
 *   vpx scripts/demos/capture.ts --showcase     # 只重做展示拼图
 *
 * 环境变量：`DEMO_CHROME` 覆盖浏览器路径，`DEMO_BASE` 跳过内建静态服务、
 * 直接对已有 origin 取值（例如 `vp dev` 起的 dev server）。
 */
import { execFile } from "node:child_process";
import { createServer } from "node:http";
import { existsSync, readFileSync, statSync } from "node:fs";
import { mkdtemp, mkdir, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join, resolve } from "node:path";
import { promisify } from "node:util";
import { chromium, type Browser, type Page } from "playwright-core";

const run = promisify(execFile);

const ROOT = resolve(import.meta.dirname, "../..");
const DIST = join(ROOT, "dist");
const OUT = join(ROOT, "demos");
const CHROME =
  process.env.DEMO_CHROME ?? "/home/rinne/.cache/ms-playwright/chromium-1223/chrome-linux64/chrome";

/** 手机竖屏：与仓库既有 QA 画布一致，底栏与药丸指示器都能落到实位。 */
const PHONE = { width: 412, height: 915 };
/** 桌面：宽屏 rail + 多列布局。 */
const DESKTOP = { width: 1440, height: 900 };

/**
 * 一张要用到的静态资源的目标宽度（px），与 README 里的展示宽度是 2 倍关系。
 * 手机图展示 240、桌面图展示 600，展示拼图另说。
 */
const TARGET = { phone: 480, desktop: 1200, showcase: 1400 } as const;

type Form = "phone" | "desktop";

type Scene = {
  /** 输出文件名前缀。 */
  slug: string;
  /** 应用内路径，可带 query；会拼到 origin 后面。 */
  path: string;
  /** 桌面图滚到底再截，长页面也能一次看全。 */
  fullPage?: boolean;
  /** 截图前等这个选择器出现；默认等 `.page-surface`。 */
  waitFor?: string;
  /**
   * 该页真实数据到达的标志。像课表 / 选课这种由 Channel 推数据的页面，
   * `.page-surface` 早就在了，必须再等一个只有加载完成后才存在的元素，
   * 否则会截到「正在加载…」或「尚未加载课程」。
   */
  ready?: string;
};

const SCENES: Scene[] = [
  { slug: "home", path: "/", fullPage: true },
  { slug: "study", path: "/study", fullPage: true },
  {
    slug: "timetable",
    path: "/study/timetable?tab=week",
    fullPage: true,
    ready: '[data-testid="week-grid"]',
  },
  { slug: "lms", path: "/study/lms", fullPage: true },
  { slug: "signin", path: "/study/signin?tab=lms" },
  { slug: "uooc", path: "/study/uooc?tab=courses", fullPage: true },
  { slug: "oj", path: "/study/oj?tab=statement&site=npuoj&contest=ct-1&problem=0:1001" },
  {
    slug: "enrollment",
    path: "/study/enrollment",
    fullPage: true,
    ready: '[data-testid="catalog-courses"]',
  },
  { slug: "life", path: "/life", fullPage: true },
  { slug: "network", path: "/life/campus-network", fullPage: true },
  { slug: "electricity", path: "/life/campus-payments/electricity", fullPage: true },
  { slug: "settings", path: "/settings?tab=appearance", fullPage: true },
];

/** 展示拼图用到的手机图（顺序即排列顺序）。 */
const SHOWCASE_PHONES = ["timetable", "uooc", "oj", "electricity"] as const;

/**
 * 不该出现在成品图里的文案。每一条都对应一次真实的截图事故：
 * 课表曾在「正在加载课表…」时被截下，选课曾带着「操作失败」的 toast 入库。
 */
const STUCK_MARKERS = [
  "正在加载",
  "尚未加载",
  "操作失败",
  "演示模式下不可用",
  "暂不可用",
  "获取巡课数据失败",
] as const;

const MIME: Record<string, string> = {
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".html": "text/html",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".ttf": "font/ttf",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webp": "image/webp",
};

/**
 * 直连 `vp dev` 等外部 origin；否则把 `dist/` 当成 SPA 站点。
 * 必须挂在 origin 根上，`/vendor/modern-monaco/*` 的 import map 与绝对资源
 * 路径才能解析（子路径部署会 404）。
 */
async function startStaticServer(): Promise<{ origin: string; close: () => Promise<void> }> {
  if (process.env.DEMO_BASE) {
    return { origin: process.env.DEMO_BASE.replace(/\/$/, ""), close: async () => {} };
  }
  if (!existsSync(join(DIST, "index.html"))) {
    throw new Error("dist/index.html 不存在，先跑 vp build（或加 --skip-build 前先构建）");
  }
  const server = createServer((request, response) => {
    const pathname = decodeURIComponent(new URL(request.url ?? "/", "http://localhost").pathname);
    let file = resolve(DIST, `.${pathname}`);
    // 目录穿越防护 + SPA history fallback：没有扩展名的路径交给 index.html。
    if (!file.startsWith(DIST) || !existsSync(file) || !statSync(file).isFile()) {
      file = join(DIST, "index.html");
    }
    response.setHeader("Content-Type", MIME[extname(file)] ?? "application/octet-stream");
    response.end(readFileSync(file));
  });
  await new Promise<void>((ready) => server.listen(0, "127.0.0.1", ready));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("静态服务未拿到端口");
  return {
    origin: `http://127.0.0.1:${address.port}`,
    close: () => new Promise<void>((done) => server.close(() => done())),
  };
}

/** 每页加载前注入的演示模式开关与首帧主题。 */
async function primePage(page: Page, theme: "light" | "dark") {
  await page.addInitScript((mode) => {
    try {
      window.localStorage.setItem("demo.enabled", "true");
      window.localStorage.setItem("szu.appearance-mode", mode);
      // 选课页会记住上次停留的子 tab，固定成默认值，截图才可复现。
      window.localStorage.removeItem("your-szu-life.campus-payments.last-route");
      window.localStorage.removeItem("oj_last_visited");
    } catch {
      // 无 localStorage 时截出来会是空页面，交给调用方的断言兜底。
    }
  }, theme);
}

/**
 * 等页面真正静止：字体就绪、图片解码完成、DOM 连续 300ms 无变动。
 * material 的组件会先渲染 shadow root 再填内容，只看 load 事件不够。
 *
 * 只等 `waitFor` 选择器出现是不够的：`.page-surface` 在数据到达前就已经挂上，
 * 课表、选课这类靠 Channel 推数据的页面会在「正在加载…」时就截到。因此还要
 * 等 `ready` 选择器出现（该页真实内容），并检查没有残留的错误文案。
 */
async function settle(page: Page, waitFor = ".page-surface", ready?: string) {
  await page.waitForSelector(waitFor, { timeout: 30_000 });
  if (ready) await page.waitForSelector(ready, { timeout: 30_000 });
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all(
      Array.from(document.images)
        .filter((image) => !image.complete)
        .map(
          (image) =>
            new Promise<void>((done) => {
              image.addEventListener("load", () => done(), { once: true });
              image.addEventListener("error", () => done(), { once: true });
            }),
        ),
    );
  });
  await page.evaluate(
    () =>
      new Promise<void>((quiet) => {
        let timer = window.setTimeout(() => quiet(), 300);
        const observer = new MutationObserver(() => {
          window.clearTimeout(timer);
          timer = window.setTimeout(() => {
            observer.disconnect();
            quiet();
          }, 300);
        });
        observer.observe(document.body, {
          subtree: true,
          childList: true,
          attributes: true,
          characterData: true,
        });
      }),
  );
  // 让入场过渡（M3 fade-through / push 动画）走完，避免截到半透明的中间帧。
  await page.waitForTimeout(400);
}

/** PNG（2 倍像素密度）→ 降采样 → WebP，一步落在 `demos/`。 */
async function toWebp(pngPath: string, slug: string, width: number) {
  const target = join(OUT, `${slug}.webp`);
  await run("magick", [
    pngPath,
    "-filter",
    "Lanczos",
    "-resize",
    `${width}x>`,
    "-quality",
    "82",
    "-define",
    "webp:method=6",
    "-define",
    "webp:alpha-compression=0",
    target,
  ]);
  return target;
}

async function captureScene(
  browser: Browser,
  origin: string,
  scene: Scene,
  form: Form,
  theme: "light" | "dark",
  scratch: string,
) {
  const viewport = form === "phone" ? PHONE : DESKTOP;
  const context = await browser.newContext({
    viewport,
    // 先按 2 倍像素密度截，降采样相当于超采样；输出尺寸再压回展示宽度的 2 倍。
    deviceScaleFactor: 2,
    locale: "zh-CN",
    timezoneId: "Asia/Shanghai",
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  try {
    await primePage(page, theme);
    await page.goto(`${origin}${scene.path}`, { waitUntil: "domcontentloaded" });
    await settle(page, scene.waitFor, scene.ready);
    // 兜底：截图上不该出现加载中、空状态或报错文案。
    const stuck = await page.evaluate((markers) => {
      const text = document.body.innerText;
      return markers.filter((marker) => text.includes(marker));
    }, STUCK_MARKERS);
    if (stuck.length) {
      throw new Error(`${scene.slug} 疑似没加载完，页面上仍有：${stuck.join("、")}`);
    }
    // 桌面长页面用 fullPage；手机用视口高度，保住固定底栏与药丸的实位。
    const fullPage = form === "desktop" && scene.fullPage !== false;
    const png = join(scratch, `${scene.slug}-${form}${theme === "dark" ? "-dark" : ""}.png`);
    await page.screenshot({ path: png, fullPage });
    const suffix = `${form === "phone" ? "mobile" : "desktop"}${theme === "dark" ? "-dark" : ""}`;
    const target = await toWebp(png, `${scene.slug}-${suffix}`, TARGET[form]);
    const size = statSync(target).size;
    if (errors.length) {
      throw new Error(`${scene.slug} 有页面错误：${errors.join(" | ")}`);
    }
    console.log(
      `  ${scene.slug}-${suffix}.webp  ${(size / 1024).toFixed(0)}KB  ${viewport.width}×${viewport.height}→${TARGET[form]}w`,
    );
  } finally {
    await context.close();
  }
}

/**
 * 展示拼图：把几张成品图按 MD3 圆角卡片排好再截一张。
 *
 * 图片以 file:// 引入（`setContent` 的 baseURL），因此不需要额外服务；
 * 拼图本身也走同一条 webp 转码路径。
 */
/** 图片以 data URL 内联：`about:blank` 页面加载 `file://` 子资源会被浏览器拦掉。 */
async function dataUrlOf(name: string) {
  const bytes = await readFile(join(OUT, name));
  return `data:image/webp;base64,${bytes.toString("base64")}`;
}

async function captureShowcase(browser: Browser, scratch: string) {
  const photos: Record<string, string> = {};
  for (const slug of ["home-desktop.webp", ...SHOWCASE_PHONES.map((s) => `${s}-mobile.webp`)]) {
    photos[slug] = await dataUrlOf(slug);
  }
  const figure = (file: string, caption: string) =>
    `<figure><img src="${photos[file]}" alt="${file}"><figcaption>${caption}</figcaption></figure>`;

  const html = `<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><style>
  :root {
    --surface: #f8f7ff;
    --surface-container: #eeeaff;
    --outline: #c9c4d8;
    --primary: #5d5fef;
    --on-surface: #1b1b21;
    --on-surface-variant: #46464f;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; padding: 40px 48px 44px;
    background:
      radial-gradient(1200px 520px at 78% -10%, #e6e0ff 0%, transparent 62%),
      var(--surface);
    color: var(--on-surface);
    font-family: "MiSans", "Noto Sans SC", system-ui, sans-serif;
    display: flex; flex-direction: column; gap: 26px;
  }
  header { display: flex; align-items: baseline; gap: 14px; }
  h1 { margin: 0; font-size: 34px; letter-spacing: -.02em; }
  header p { margin: 0; font-size: 15px; color: var(--on-surface-variant); }
  .grid { display: grid; grid-template-columns: 1.62fr 1fr; gap: 22px; }
  .column { display: flex; flex-direction: column; gap: 22px; min-width: 0; }
  .phones { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 22px; }
  figure {
    margin: 0; background: var(--surface-container);
    border: 1px solid var(--outline); border-radius: 28px;
    padding: 14px; display: flex; flex-direction: column; gap: 10px;
    box-shadow: 0 1px 2px rgba(20, 16, 44, .06), 0 12px 28px -18px rgba(20, 16, 44, .35);
  }
  figure > img { width: 100%; display: block; border-radius: 18px; }
  .wide > img { border-radius: 20px; }
  figcaption { font-size: 13px; color: var(--on-surface-variant); padding-inline: 4px; }
  figcaption b { color: var(--primary); font-weight: 600; }
  /* 两张手机图按真实截图比例定宽，行高不再依赖图片加载完成的先后。 */
  .phones figure { width: 100%; }
  .phones img { aspect-ratio: 412 / 915; object-fit: cover; object-position: top; }
</style></head>
<body>
  <header><h1>泥深 · Your SZU Life</h1>
    <p>覆盖课表、作业、签到与校园生活的一站式客户端</p></header>
  <main id="sheet" class="grid">
    <figure class="wide">
      ${figure("home-desktop.webp", "<b>首页</b> · 公告、今日课表与全部截止待办一屏看完")}
    </figure>
    <div class="column">
      <div class="phones">
        ${SHOWCASE_PHONES.slice(0, 2)
          .map((slug) => figure(`${slug}-mobile.webp`, CAPTIONS[slug]))
          .join("\n        ")}
      </div>
      <div class="phones">
        ${SHOWCASE_PHONES.slice(2)
          .map((slug) => figure(`${slug}-mobile.webp`, CAPTIONS[slug]))
          .join("\n        ")}
      </div>
    </div>
  </main>
</body></html>`;

  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 2,
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  try {
    await page.setContent(html, { waitUntil: "load", baseURL: `${OUT}/` });
    await page.evaluate(() => document.fonts.ready);
    await page.evaluate(async () => {
      await Promise.all(
        Array.from(document.images).map((image) =>
          image.complete ? undefined : image.decode().catch(() => undefined),
        ),
      );
    });
    // 拼图高度由内容决定：等图片解码后量一次 `#sheet`，再按实际尺寸截。
    await page.waitForFunction(() => document.fonts.status === "loaded");
    await page.evaluate(async () => {
      await Promise.all(
        Array.from(document.images).map((image) =>
          image.complete ? undefined : image.decode().catch(() => undefined),
        ),
      );
    });
    const height = await page.evaluate(() => {
      const sheet = document.getElementById("sheet")!;
      // 量最后一个卡片而不是 `body.scrollHeight`：body 至少撑满视口，
      // 内容比视口矮时会多出一大块空白。
      const figures = sheet.querySelectorAll("figure");
      const last = figures[figures.length - 1]!.getBoundingClientRect();
      return Math.ceil(last.bottom + 44);
    });
    const png = join(scratch, "showcase.png");
    // `clip` 按设备像素算，用 CSS 尺寸乘上像素密度；比先改视口再截更稳，
    // 不会因为浏览器对极短视口的取整而多/少截几像素。
    await page.screenshot({
      path: png,
      clip: { x: 0, y: 0, width: 1280, height },
    });
    const target = await toWebp(png, "showcase", TARGET.showcase);
    console.log(`  showcase.webp  ${(statSync(target).size / 1024).toFixed(0)}KB`);
  } finally {
    await context.close();
  }
}

/** 各页面在拼图里的说明，与 README 的措辞保持一致。 */
const CAPTIONS: Record<string, string> = {
  timetable: "<b>课表</b> · 周视图 + 签到提示",
  uooc: "<b>UOOC</b> · 进度与考核一屏汇总",
  oj: "<b>OJ</b> · 题面、样例与在线判题",
  electricity: "<b>电量</b> · 用量走势与余额预测",
};

function parseArgs(argv: string[]) {
  const onlyIndex = argv.indexOf("--only");
  return {
    only: onlyIndex >= 0 ? argv[onlyIndex + 1] : undefined,
    skipBuild: argv.includes("--skip-build"),
    showcase: argv.includes("--showcase"),
    dark: argv.includes("--dark"),
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));

  if (!existsSync(CHROME)) {
    throw new Error(`找不到浏览器：${CHROME}（用 DEMO_CHROME 指定）`);
  }
  if (!args.skipBuild) {
    console.log("构建前端产物 (vp build)…");
    await run("vp", ["build"], { cwd: ROOT, env: process.env });
  }
  await mkdir(OUT, { recursive: true });

  const { origin, close } = await startStaticServer();
  const scratch = await mkdtemp(join(tmpdir(), "szu-demos-"));
  const browser = await chromium.launch({
    executablePath: CHROME,
    args: ["--no-sandbox", "--disable-dev-shm-usage", "--font-render-hinting=none"],
  });

  try {
    console.log(`取值 origin：${origin}`);
    const themes: ("light" | "dark")[] = args.dark ? ["light", "dark"] : ["light"];

    if (args.showcase) {
      console.log("生成展示拼图…");
      await captureShowcase(browser, scratch);
      return;
    }

    const scenes = args.only ? SCENES.filter((scene) => scene.slug === args.only) : SCENES;
    if (!scenes.length) {
      throw new Error(
        `没有匹配的场景：${args.only}（可选 ${SCENES.map((s) => s.slug).join(", ")}）`,
      );
    }

    for (const theme of themes) {
      for (const scene of scenes) {
        for (const form of ["phone", "desktop"] as const) {
          await captureScene(browser, origin, scene, form, theme, scratch);
        }
      }
    }

    if (!args.only) {
      console.log("生成展示拼图…");
      await captureShowcase(browser, scratch);
    }
  } finally {
    await browser.close();
    await close();
    await rm(scratch, { recursive: true, force: true });
  }

  const outputs = (await readdir(OUT)).filter((name) => name.endsWith(".webp")).sort();
  const total = outputs.reduce((sum, name) => sum + statSync(join(OUT, name)).size, 0);
  console.log(`\n共 ${outputs.length} 张 webp，合计 ${(total / 1024).toFixed(0)}KB → demos/`);
  const leftovers = (await readdir(OUT)).filter((name) => !name.endsWith(".webp"));
  if (leftovers.length) console.warn(`注意：demos/ 里还有非 webp 文件：${leftovers.join(", ")}`);
}

await main();
