/** 设备平台展示名：Rust 侧的 `std::env::consts::OS` 到用户可读文案。 */
export function platformLabel(platform: string) {
  switch (platform) {
    case "android":
      return "Android";
    case "windows":
      return "Windows";
    case "macos":
      return "macOS";
    case "linux":
      return "Linux";
    case "ios":
      return "iOS";
    default:
      return platform || "未知平台";
  }
}
