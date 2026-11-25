/** 远程授权登录的 Tauri 命令包装。密码只在 Rust 侧与一次性返回值中出现。 */
import { invoke } from "@tauri-apps/api/core";
import type { PairCredentials, PairHostView, PairPeerView } from "./types";

/** host（未登录的新设备）开始一个配对房间。 */
export function pairLoginHostStart() {
  return invoke<PairHostView>("pair_login_host_start");
}

export function pairLoginHostCancel() {
  return invoke<void>("pair_login_host_cancel");
}

/** 一次性取走授权端发来的凭据；取走后 host 会话即被销毁。 */
export function pairLoginTakeCredentials() {
  return invoke<PairCredentials | null>("pair_login_take_credentials");
}

/** 接受一条配对链接（深链或设置页粘贴/扫码）。secret 只留在 Rust 内存。 */
export function pairLoginAcceptUrl(url: string) {
  return invoke<void>("pair_login_client_accept_url", { url });
}

/** 加入配对网络并拉取新设备信息。 */
export function pairLoginConnect() {
  return invoke<PairPeerView>("pair_login_client_connect");
}

export function pairLoginAuthorize() {
  return invoke<void>("pair_login_client_authorize");
}

export function pairLoginCancel() {
  return invoke<void>("pair_login_client_cancel");
}
