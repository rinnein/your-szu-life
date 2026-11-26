# WorkBuddy 专家接入

`workbuddy/plugins/your-szu-life/` 是「泥深助手」专家包的源码。它把 WorkBuddy 与本机泥深桌面端连接起来：

```text
WorkBuddy 专家
  ├─ agents/szu-life-assistant.md   角色、首启流程、工具边界
  ├─ skills/your-szu-life/          运行手册 + Windows/Linux 安装说明
  ├─ bin/szu-life.mjs               状态机 / 安装器 / MCP 客户端 / stdio 桥
  └─ .mcp.json                      stdio MCP（node ... szu-life.mjs mcp）
        ├─ 读 app_data_dir/integration.json
        ├─ 探测 127.0.0.1:53901..54000，用 initialize serverInfo 校验
        └─ 代理到泥深 http://127.0.0.1:<port>/mcp（Streamable HTTP）
```

桥始终能启动：泥深未安装或 MCP 未开启时，专家仍可对话，并通过 `szu_life_status` / `szu_life_ensure_ready` 引导用户；应用就绪后再代理全部工具与资源。

## 构建与测试

```bash
vp run workbuddy:test
vp run workbuddy:build
```

- 测试覆盖：manifest/头像/ZIP 校验、Linux 发行版分流、deb/AUR 命令生成、发布选择、下载候选、`integration.json` 解析、mock Streamable HTTP MCP、stdio 桥转发。
- 产物：`workbuddy/dist/your-szu-life-workbuddy-<version>.zip`，顶层单目录 `your-szu-life/`。
- 打包脚本会校验：`displayDescription.zh` 40–50 字、tags/quickPrompts 各 3 条、`defaultInitPrompt === quickPrompts[0]`、avatar 512×512 且 ≤500KB、agent/skill frontmatter、`.mcp.json` 图标、桥版本与 manifest 一致、包内无明文 token。

## Linux 安装分流（硬规则）

| 发行版                                   | 行为                                                                                              |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Debian/Ubuntu 等                         | `szu-life install` 返回 bash 脚本（下载 deb + `sudo apt install`），由用户执行；agent 不执行 sudo |
| Arch/Manjaro 等                          | 返回 AUR 代码块（`your-szu-life-bin`，provides `your-szu-life`），由用户执行；AUR 跟随 canary     |
| 其它（Fedora/openSUSE/NixOS/SteamOS 等） | agent 执行 `szu-life install --yes`，把 AppImage 安装到 XDG 用户目录，无 sudo                     |

Windows 走 MSI，经 UAC 静默安装。所有路径都先征得用户同意。

## 应用侧契约

- `app_data_dir/integration.json`：由泥深在启动/MCP 开关/初始化/认证/UOOC 事件时写入，Unix 0600，不含 PII。
- `--silent`：仅本次启动隐藏窗口，不写偏好；第二实例不会抢焦点。
- MCP 工具 `integration_get_status` 与资源 `your-szu-life://data/app/integration`。
- 深链：`your-szu-life://page/settings?tab=mcp`、`your-szu-life://page/`、`your-szu-life://page/study/uooc`。
- 应用运行期会调用 deep-link `register_all()`；Linux 安装器另写 `.desktop` 的 `%u` + `MimeType`，Windows 安装器写 HKCU URL protocol 兜底。

## 安全

- MCP 仅回环；桥不监听端口。
- 桥只读取状态文件与调用 127.0.0.1 MCP；不读 keyring/vault。
- 安装校验 ELF/OLE 魔数与大小；CNB/GitHub 提供 SHA-256 时校验。
- Debian/Arch 的提权命令由用户执行；MSI 走系统 UAC。
- 包内不允许硬编码 token；构建脚本会扫描常见密钥形态。

## 手动验收（本机无法自动覆盖）

1. WorkBuddy 导入 ZIP → 召唤专家 → 桥连接成功，`szu_life_status` 返回。
2. Arch：`szu-life install` 只输出 AUR 代码块，不执行；用户安装后 `pacman -Qo /usr/bin/your-szu-life`。
3. Debian：输出 deb 代码块；用户执行后 `dpkg-query -S /usr/bin/your-szu-life`。
4. 其它 Linux：临时 `XDG_DATA_HOME` 下 AppImage 安装、`--silent`、深链、MCP 开关、向导、UOOC 登录、真实工具调用。
5. Windows：MSI + UAC、Program Files、HKCU 协议注册、升级前退出托盘。
6. 专家卡片、头像、双语字段在 WorkBuddy 中的实际渲染。

## 发布

1. 构建 ZIP；
2. 登录 WorkBuddy 开放平台，在「我的专家」上传 ZIP；
3. 按平台要求填写展示信息（已在本包 manifest 中）；
4. 审核通过后上架。

本仓库不自动发布 WorkBuddy 市场；`workbuddy/dist/` 已被 gitignore。
