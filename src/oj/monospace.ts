/**
 * OJ 模块统一的等宽字体栈，取 Monaco Editor 的内置默认字体栈。
 *
 * 样例框（SampleField）的文本层与空白标记层必须与代码编辑器使用同一字体栈，
 * 才能在不同系统/浏览器上逐字符对齐；标记层的 advance 全部用 `ch` 单位强制，
 * 不依赖具体字形的宽度。不要在本模块内再硬编码其它等宽字体栈。
 */
export const OJ_MONO_FONT_FAMILY =
  "'SF Mono', Monaco, Menlo, Consolas, 'Liberation Mono', 'Courier New', monospace";
