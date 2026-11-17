/**
 * OJ 模块的等宽字体栈，即全应用共用的 Monaspace Neon 栈（见 src/monospace.ts）。
 *
 * 样例框（SampleField）的文本层与空白标记层必须与代码编辑器使用同一字体栈，
 * 才能在不同系统/浏览器上逐字符对齐；标记层的 advance 全部用 `ch` 单位强制，
 * 不依赖具体字形的宽度。字体栈只在 src/monospace.ts 定义一次，这里只做转出。
 */
export { MONO_FONT_FAMILY as OJ_MONO_FONT_FAMILY } from "../monospace";
