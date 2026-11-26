# 排障手册

## 应用已安装但没反应

- 泥深关闭按钮是「收纳到托盘」，不是退出。先 `szu-life status` 看 `running`。
- 未运行：`szu-life ensure` 会用 `--silent` 启动；需要窗口时 `szu-life open home`。
- 托盘也没有：检查进程 `your-szu-life`，或让用户从开始菜单/应用菜单打开。

## 状态文件缺失或过旧

- 新版本会写 `app_data_dir/integration.json`；旧版本没有。
- 此时桥会改用 MCP 的 `onboarding_get_status`、`auth_get_status` 与 `data/uooc/courses` 兜底。
- 如果 MCP 也没开，`nextAction` 是 `enable-mcp`；打开设置页让用户开启。
- 长期使用旧版本建议 `szu-life install --yes`（其它 Linux/Windows）或按发行版更新。

## MCP 未开启

- 打开 `your-szu-life://page/settings?tab=mcp`，让用户打开「开启 MCP 服务端口」。
- 开关在「设置 → MCP」。开启后状态文件与端口会更新。
- 不要尝试绕过开关；这是用户的安全选择。

## MCP 端口不可达

- 端口在 53901–54000 动态分配；不要假定 53901。
- `szu-life status` 的 `mcp.url` 是探测结果；`nextAction=mcp-unreachable` 时让用户看设置页的端口提示。
- 端口被占用时应用会切换并弹提示；确认后重查。
- 端口段耗尽时应用会弹模态框；让用户释放端口或重启应用。

## 初始化向导 / 统一认证

- 初始化未完成或认证失效时，所有页面都会被向导挡住。
- 打开泥深窗口，让用户在向导里完成；不要在对话里索要密码。
- 完成后 `szu-life status` 会显示 `onboarding.needsSetup=false` 且 `auth.loggedIn=true`。

## UOOC 未登录

- `/study/uooc` 页面会显示「需要登录」横幅与「登录 UOOC」按钮。
- 点击后是官方登录窗口；用户在官方窗口完成，凭据不经过专家。
- 登录成功后 `uooc.loggedIn=true`；否则重试或检查网络。

## Debian/Arch 安装命令

- Debian：用户执行 `szu-life install` 返回的脚本；需要 sudo。失败时检查 `curl`/`wget`、apt 源与磁盘空间。
- Arch：用户执行 `yay`/`paru`/`makepkg`；AUR 包名 `your-szu-life-bin`（provides `your-szu-life`）。AUR 跟随 canary。
- 用户拒绝 sudo/AUR 时停止，不要改用别的提权方式。

## Windows MSI / UAC

- `szu-life install --yes` 会弹 UAC；拒绝后无法静默安装。
- 升级前让用户从托盘退出，避免文件占用。
- `msiexec` 3010 表示需要重启。
- 未签名 MSI 可能触发 SmartScreen；不要建议关闭系统安全功能。
- 安装后找不到 exe：检查 `Program Files\your-szu-life` 或 `%LOCALAPPDATA%\Programs\your-szu-life`。

## AppImage / FUSE

- 没有 `libfuse2` 时安装器会写 `APPIMAGE_EXTRACT_AND_RUN=1` 到启动包装。
- 仍无法启动：改用 deb/AUR，或让用户安装 FUSE2。
- AppImage 移动后运行期协议注册会失效；重新启动一次应用即可重注册。

## 桥启动失败

- `.mcp.json` 用 `command: "node"`；WorkBuddy 桌面端自带 Node，一般可用。
- 若连接器红点：先确认 `node --version` 可用；或让用户用 `szu-life mcp-url` 拿到端点，在「连接器 → 自定义连接器」添加 HTTP：
  ```json
  {
    "mcpServers": {
      "your-szu-life": {
        "type": "http",
        "url": "http://127.0.0.1:<port>/mcp"
      }
    }
  }
  ```
- 工具列表刚连上时可能没刷新；用 `szu_life_list_tools` / `szu_life_call`，或让用户新开一次会话。

## 网络/下载

- 安装源顺序：CNB → GitHub 加速 → GitHub。CNB 不通时检查网络/代理。
- 代理环境：`curl`/PowerShell 继承系统代理；Node 兜底不继承代理。
- 下载校验失败：重试；CNB 提供 SHA-256，说明文件损坏或被篡改。

## 隐私

- `integration.json` 不含姓名、学号、Cookie、密码或密钥；只含状态、端口、版本与布尔值。
- `szu-life status` 不输出本机路径。
- 工具返回的个人资料按需展示，不要主动扩散。
