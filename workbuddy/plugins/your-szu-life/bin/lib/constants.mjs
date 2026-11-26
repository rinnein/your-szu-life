// Shared constants for the your-szu-life WorkBuddy expert bridge.

export const EXPERT_VERSION = "1.0.0";
export const INSTALL_META_SCHEMA_VERSION = 1;

export const APP_ID = "in.rinne.your-szu-life";
export const APP_NAME = "泥深";
export const APP_BINARY = "your-szu-life";
export const APP_SCHEME = "your-szu-life";

export const REPO = "rinnein/your-szu-life";
export const CNB_REPO = "rinnein/szu/your-szu-life";
export const AUR_PACKAGE = "your-szu-life-bin";

export const GITHUB_RELEASES_PAGE = `https://github.com/${REPO}/releases`;
export const GITHUB_RELEASES_API = `https://api.github.com/repos/${REPO}/releases?per_page=20`;
export const CNB_RELEASES_API = `https://cnb.cool/${CNB_REPO}/-/releases`;
export const GITHUB_ACCELERATORS = ["https://gh-proxy.com/", "https://ghfast.top/"];

export const MCP_PORT_RANGE = [53901, 54000];
export const MCP_PROTOCOL_VERSION = "2025-06-18";
export const MCP_LEGACY_PROTOCOL_VERSION = "2025-03-26";

export const DEEP_LINKS = {
  home: `${APP_SCHEME}://page/`,
  settings: `${APP_SCHEME}://page/settings`,
  mcpSettings: `${APP_SCHEME}://page/settings?tab=mcp`,
  uooc: `${APP_SCHEME}://page/study/uooc`,
};

export const INSTALL_MARKER = "# managed by the your-szu-life WorkBuddy expert";
export const MCP_BRIDGE_NAME = "your-szu-life-workbuddy";

export const TMP_PREFIX = "szu-life-";
