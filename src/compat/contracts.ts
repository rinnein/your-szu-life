/** 构建插件与运行时共享的 CSS 回退契约；不依赖 DOM 或构建工具。 */
export const WEBVIEW_TARGET = "chrome90";
export const SHEET_METADATA = "--szu-compat-sheet";
export function compatId(value: string): string {
  let hash = 2166136261;
  for (let i = 0; i < value.length; i++) hash = Math.imul(hash ^ value.charCodeAt(i), 16777619);
  return (hash >>> 0).toString(36);
}
export function normalizeMix(value: string): string {
  return value
    .replace(/\s+/g, " ")
    .replace(/\s*([(),])\s*/g, "$1")
    .trim();
}
export function mixProperty(value: string): string {
  return `--szu-compat-color-${compatId(normalizeMix(value))}`;
}
export interface HasRule {
  attribute: string;
  candidate: string;
  conditions: string[];
}
export interface MixRule {
  selector: string;
  pseudo: string;
  source: string;
  target: string;
}
export interface ContainerRule {
  name: string;
  attribute: string;
  min?: number;
  max?: number;
}
export interface ContainerOwner {
  selector: string;
  name: string;
}
export interface CompatSheet {
  has: HasRule[];
  mix: MixRule[];
  containers: ContainerRule[];
  owners: ContainerOwner[];
}
