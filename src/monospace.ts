/**
 * 全应用唯一的等宽字体栈：内置的 Monaspace Neon（githubnext/monaspace，OFL-1.1），
 * 由 src/app.tsx 从 @fontsource/monaspace-neon 加载。
 *
 * 样例框（SampleField）的文本层与空白标记层必须与代码编辑器使用同一字体栈，
 * 才能在不同系统/浏览器上逐字符对齐；标记层的 advance 全部用 `ch` 单位强制，
 * 不依赖具体字形的宽度。不要在本模块内再硬编码其它等宽字体栈，也不要把全局
 * UI 字体（MiSans，见 index.css 的 --app-font-family）插进来：MiSans 不是等宽字体，
 * 一进来 ch 就不再等于字符宽度，样例框的空白标记会错位。
 *
 * 末尾的 monospace 只兜底 Monaspace Neon 未覆盖的字符（它只有 latin 子集）。
 */
export const MONO_FONT_FAMILY = "'Monaspace Neon', ui-monospace, monospace";

/** OJ 模块沿用的名字，值即全应用共用的等宽栈。 */
export const OJ_MONO_FONT_FAMILY = MONO_FONT_FAMILY;
