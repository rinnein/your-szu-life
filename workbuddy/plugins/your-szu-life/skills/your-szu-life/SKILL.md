---
name: your-szu-life
display_name: 泥深接入
display_name_en: Your SZU Life Integration
description: Install, connect, and troubleshoot the Your SZU Life desktop app and its local MCP tools.
description_zh: 安装并连接泥深桌面端，处理 MCP、初始化、登录与分发行版安装。
description_en: Install and connect the Your SZU Life desktop app, including MCP, onboarding, login, and distro-specific installation.
version: 1.0.0
author: Rinne
---

# 泥深接入操作手册

这个 Skill 是「泥深助手」专家的运行手册。命令统一由插件 `bin/` 提供，在 Bash 中可直接执行 `szu-life ...`；Windows 若直接调用失败，可用 `cmd /c szu-life ...`。

## 状态机

先 `szu-life status`，按 `nextAction` 行动：

| nextAction        | 含义                                | 动作                                      |
| ----------------- | ----------------------------------- | ----------------------------------------- |
| `unsupported`     | 非 Windows/Linux x64 或没有桌面会话 | 解释并停止                                |
| `install`         | 未安装                              | 征得同意后按发行版安装                    |
| `launch`          | 已安装但未运行                      | `szu-life ensure` 会静默启动              |
| `onboarding`      | 初始化向导未完成                    | 打开首页，让用户完成向导                  |
| `auth`            | 统一认证未完成/失效                 | 打开首页，让用户在官方认证窗口登录        |
| `enable-mcp`      | MCP 开关未开启                      | 打开 `settings?tab=mcp`，让用户开启       |
| `mcp-unreachable` | 开关已开但端口探测不到              | 打开设置页检查端口提示/重开开关           |
| `uooc-login`      | 请求 UOOC 但未登录                  | 打开 `/study/uooc`，让用户点「登录 UOOC」 |
| `ready`           | 可用                                | 调用应用 MCP 工具                         |

`szu-life ensure --need uooc` 会把 UOOC 登录纳入同一次检查。`ensure` **不会安装**，最多静默启动和打开页面。

## 命令

```bash
szu-life status                 # JSON 状态
szu-life ensure [--need uooc]   # 按状态机启动/打开页面
szu-life install                # Debian/Arch: 输出 bash 命令；其它: 计划
szu-life install --yes          # 其它 Linux / Windows: 自动安装
szu-life install --dry-run      # 只解析版本与下载地址
szu-life launch [--visible]     # 默认 --silent
szu-life open mcp-settings      # home|settings|mcp-settings|uooc
szu-life mcp-list               # 列出泥深 MCP 工具
szu-life mcp-call <tool> [json] # 直接调用 MCP 工具
szu-life mcp-url                # 输出探测到的 HTTP 端点
szu-life version
```

## 安装分流（硬规则）

- **Debian 系**：`szu-life install` 返回 `script`；把 `script` 原样放进 ```bash 代码块，让用户自己执行。不要执行 `sudo`，不要替用户输入密码。用户完成后 `szu-life status` 验证。
- **Arch 系**：`szu-life install` 返回 AUR 代码块（`your-szu-life-bin`，provides `your-szu-life`）。把它原样交给用户；不要执行 `yay`/`paru`/`makepkg`。AUR 包由 CI 跟随 canary。
- **其它 Linux**：`szu-life install --yes` 自动把 AppImage 安装到 `$XDG_DATA_HOME/your-szu-life/`，启动器在 `$XDG_BIN_HOME`（默认 `~/.local/bin`），桌面项在 `$XDG_DATA_HOME/applications`，不需要 sudo。
- **Windows**：`szu-life install --yes` 下载 MSI，`msiexec` 经 UAC 静默安装；提前说明会弹 UAC。

详细说明见 `@references/install-linux.md`、`@references/install-windows.md`。

## MCP 使用

1. `szu-life mcp-list` 或 `szu_life_list_tools` 查看工具。
2. 直接调用暴露的工具；列表未刷新时用 `szu_life_call`：`{ "tool": "todos_list", "arguments": {} }`。
3. 资源用 `szu_life_read_resource`，URI 只接受 `your-szu-life://data/...`。
4. 页面跳转用 `open_page` 或 `szu-life open`。
5. 写操作先确认；读操作可直接执行。

常见工作流见 `@references/tool-workflows.md`。

## 失败处理

- 应用已关闭到托盘：`szu-life ensure` 会 `--silent` 启动；需要窗口时用 `szu-life open`。
- MCP 端口被占用：应用会自动换端口并弹提示；以 `status` 的 `mcp.url` 为准，不要假定 53901。
- MCP 开关关闭：不要在桥里尝试自动打开；打开设置页让用户点「开启 MCP 服务端口」。
- 初始化未完成：所有页面会被向导挡住；先完成向导。
- UOOC 未登录：页面会显示「登录 UOOC」按钮；让用户在官方窗口登录。
- AppImage 无法启动（缺少 libfuse2）：安装器会自动使用 `APPIMAGE_EXTRACT_AND_RUN=1`；仍失败则建议 deb 或发行版包。
- Windows UAC 被拒绝：让用户重新运行并允许管理员权限；不要绕过。
- 桥在 WorkBuddy 中启动失败（没有 Node）：用 `szu-life mcp-url` 拿到端点，在「连接器 → 自定义连接器」配置 HTTP 直连。

更多见 `@references/troubleshooting.md`。
