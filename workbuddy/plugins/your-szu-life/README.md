# 泥深助手（Your SZU Life）

这是「泥深」（your-szu-life）的 WorkBuddy 专家包。召唤后，专家会按你的平台完成安装、启动、MCP 接入与初始化引导，然后用泥深的本机 MCP 工具查询课表、成绩、宿舍电费、待办、校园网、UOOC、OJ 等。

## 支持范围

- Windows x64（MSI）
- Linux x64（Debian 系 deb / Arch 系 AUR / 其它发行版 XDG AppImage）
- 必须是本机桌面会话；不支持 Web 版 WorkBuddy 沙盒、无桌面容器、macOS、Linux arm64

## 首次使用

1. 召唤「泥深助手」，专家会先检查是否已安装。
2. 未安装时会介绍泥深并询问是否安装：
   - Debian/Ubuntu：给出一条 bash 命令，你自己执行（需要 sudo）；
   - Arch/Manjaro：给出 AUR 命令（`your-szu-life-bin`，跟随 canary）；
   - 其它 Linux：专家自动安装 AppImage 到 XDG 用户目录（无 sudo）；
   - Windows：专家自动安装 MSI，会弹 UAC。
3. 启动泥深后，按专家提示完成初始化向导 / 统一认证。
4. 如果 MCP 未开启，专家会打开「设置 → MCP」，请打开「开启 MCP 服务端口」。
5. 需要 UOOC 时，专家会打开 `/study/uooc`，请点「登录 UOOC」完成官方登录。

## 隐私

- 泥深 MCP 只监听 `127.0.0.1`，不对外网/局域网开放。
- 登录凭据保存在系统钥匙串；专家不会读取 keyring/vault，也不会索要校园卡密码。
- 专家读取的状态文件 `integration.json` 只含版本、MCP 端口/开关、初始化与登录布尔值，不含姓名、学号、Cookie 或密钥。

## HTTP 直连兜底

如果 WorkBuddy 无法启动内置 Node stdio 桥（例如 `node` 不在 PATH），可以：

1. 运行 `szu-life mcp-url` 获取端点；
2. 在 WorkBuddy 的「专家 · 技能 · 连接器 → 连接器 → 自定义连接器」中添加：

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

3. 点「信任」，必要时重启 WorkBuddy。

## 开发

源码在仓库 `workbuddy/` 下：

```bash
vp run workbuddy:test
vp run workbuddy:build
```

构建产物：`workbuddy/dist/your-szu-life-workbuddy-<version>.zip`，上传到 WorkBuddy 开放平台即可。
