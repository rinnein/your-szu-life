/**
 * 演示模式的本地开关。
 *
 * demo shim 需要在任意 invoke 之前同步知道当前是否处于演示模式，因此用
 * localStorage 做镜像：`set_demo_mode` 命令成功返回后由 shim 更新；应用启动时
 * 同步读取。后端的 `demo.enabled` 偏好仍是权威状态，这里只是前端侧的快速镜像，
 * 不可用时退回内存态（仅当次会话有效）。
 */

const STORAGE_KEY = "demo.enabled";

let active = readStored();

function readStored(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

export function demoActive(): boolean {
  return active;
}

export function setDemoActive(value: boolean): void {
  active = value;
  try {
    if (value) window.localStorage.setItem(STORAGE_KEY, "true");
    else window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    // localStorage 不可用时只保留内存态。
  }
}
