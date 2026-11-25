/** 远程授权登录的前后端契约（Rust 侧 camelCase 序列化）。 */

export type PairDevice = {
  /** 设备名（Android 可能是型号，桌面是 hostname）。 */
  name: string;
  /** `std::env::consts::OS`，如 android / linux / windows。 */
  platform: string;
  appVersion: string;
};

export type PairHostStatus = "waiting" | "connected" | "accepted" | "expired";

export type PairHostView = {
  status: PairHostStatus;
  /** 只用于两端肉眼核对的 6 位码。 */
  code: string;
  /** 二维码内容：your-szu-life://page/pair-login?secret=... */
  deepLink: string;
  expiresAtMs: number;
};

export type PairPeerView = {
  code: string;
  device: PairDevice;
};

export type PairCredentials = {
  campusCardNumber: string;
  password: string;
};
